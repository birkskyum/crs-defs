//! Lists every projected and geographic 2D CRS of each authority, asks PROJ for its candidate
//! transformations to WGS 84, writes each one as a proj4 string, and records reference positions so
//! that proj4js can be checked against PROJ. Output goes to `data/raw/<authority>/<code>.json`.

use proj_sys::*;
use serde_json::{Value, json};
use std::ffi::{CStr, CString, c_char};
use std::fs;
use std::path::PathBuf;
use std::ptr::{null, null_mut};
use std::sync::Mutex;
use std::sync::atomic::{AtomicUsize, Ordering};

const MAX_CANDIDATES: i32 = 12;
const GRID_CDN: &str = "https://cdn.proj.org/";
const SAMPLE_FRACTIONS: [f64; 4] = [0.1, 0.37, 0.63, 0.9];
const LONGITUDE_ROTATION: &str = "9601";
const AUTHORITIES: [&str; 2] = ["EPSG", "ESRI"];
/// Ellipsoid names proj4js 2.22 does not know; it falls back to WGS 84 for them without a warning.
const PROJ4JS_MISSING_ELLIPSOIDS: [(&str, &str); 2] = [("WGS72", "+a=6378135 +rf=298.26"), ("GSK2011", "+a=6378136.5 +rf=298.2564151")];

struct Context(*mut PJ_CONTEXT);

impl Context {
    fn new() -> Self {
        unsafe {
            let ctx = proj_context_create();
            proj_log_level(ctx, PJ_LOG_LEVEL_PJ_LOG_NONE);
            proj_context_set_enable_network(ctx, 1);
            Context(ctx)
        }
    }
}

impl Drop for Context {
    fn drop(&mut self) {
        unsafe { proj_context_destroy(self.0) };
    }
}

struct Obj(*mut PJ);

impl Obj {
    fn new(ptr: *mut PJ) -> Option<Obj> {
        (!ptr.is_null()).then_some(Obj(ptr))
    }
}

impl Drop for Obj {
    fn drop(&mut self) {
        unsafe { proj_destroy(self.0) };
    }
}

fn text(ptr: *const c_char) -> Option<String> {
    (!ptr.is_null()).then(|| unsafe { CStr::from_ptr(ptr) }.to_string_lossy().into_owned())
}

fn c(s: &str) -> CString {
    CString::new(s).unwrap()
}

fn name_of(obj: &Obj) -> String {
    text(unsafe { proj_get_name(obj.0) }).unwrap_or_default()
}

fn id_of(obj: &Obj) -> Option<String> {
    let auth = text(unsafe { proj_get_id_auth_name(obj.0, 0) })?;
    let code = text(unsafe { proj_get_id_code(obj.0, 0) })?;
    Some(format!("{auth}:{code}"))
}

fn proj4_of(ctx: &Context, obj: &Obj) -> Option<String> {
    let s = text(unsafe { proj_as_proj_string(ctx.0, obj.0, PJ_PROJ_STRING_TYPE_PJ_PROJ_4, null()) })?;
    Some(s.split_whitespace().filter(|t| *t != "+type=crs").collect::<Vec<_>>().join(" "))
}

fn create(ctx: &Context, definition: &str) -> Option<Obj> {
    let def = c(definition);
    Obj::new(unsafe { proj_create(ctx.0, def.as_ptr()) })
}

fn from_database(ctx: &Context, authority: &str, code: &str) -> Option<Obj> {
    let (auth, code) = (c(authority), c(code));
    Obj::new(unsafe { proj_create_from_database(ctx.0, auth.as_ptr(), code.as_ptr(), PJ_CATEGORY_PJ_CATEGORY_CRS, 0, null()) })
}

fn codes(ctx: &Context, authority: &str, kind: PJ_TYPE) -> Vec<String> {
    let auth = c(authority);
    let mut out = Vec::new();
    unsafe {
        let list = proj_get_codes_from_database(ctx.0, auth.as_ptr(), kind, 0);
        let mut i = 0;
        while !list.is_null() && !(*list.add(i)).is_null() {
            out.push(text(*list.add(i)).unwrap());
            i += 1;
        }
        proj_string_list_destroy(list);
    }
    out.sort_by_key(|s| s.parse::<u64>().unwrap_or(u64::MAX));
    out
}

/// A transformation in each CRS's own axis order: latitude first for EPSG:4326, the `+axis` order for a proj4 string.
fn transformer(ctx: &Context, from: &Obj, to: &Obj) -> Option<Obj> {
    Obj::new(unsafe { proj_create_crs_to_crs_from_pj(ctx.0, from.0, to.0, null_mut(), null()) })
}

fn apply(op: &Obj, x: f64, y: f64) -> Option<[f64; 2]> {
    unsafe {
        proj_errno_reset(op.0);
        let out = proj_trans(op.0, PJ_DIRECTION_PJ_FWD, PJ_COORD { v: [x, y, 0.0, f64::INFINITY] });
        let [a, b, ..] = out.v;
        (proj_errno(op.0) == 0 && a.is_finite() && b.is_finite() && a.abs() < 1e12 && b.abs() < 1e12).then_some([a, b])
    }
}

/// Applies an operation whose output is EPSG:4326 and returns longitude, latitude.
fn apply_to_lonlat(op: &Obj, x: f64, y: f64) -> Option<[f64; 2]> {
    apply(op, x, y).map(|[lat, lon]| [lon, lat])
}

fn distance([lon1, lat1]: [f64; 2], [lon2, lat2]: [f64; 2]) -> f64 {
    let r = std::f64::consts::PI / 180.0;
    let a = ((lat2 - lat1) * r / 2.0).sin().powi(2) + (lat1 * r).cos() * (lat2 * r).cos() * ((lon2 - lon1) * r / 2.0).sin().powi(2);
    2.0 * 6371008.8 * a.sqrt().min(1.0).asin()
}

fn sample_points([w, s, e, n]: [f64; 4]) -> Vec<[f64; 2]> {
    let east = if e >= w { e } else { e + 360.0 };
    let mut fractions = vec![[0.5, 0.5]];
    for fx in SAMPLE_FRACTIONS {
        for fy in SAMPLE_FRACTIONS {
            fractions.push([fx, fy]);
        }
    }
    fractions
        .into_iter()
        .map(|[fx, fy]| {
            let lon = w + (east - w) * fx;
            [(lon + 540.0).rem_euclid(360.0) - 180.0, s + (n - s) * fy]
        })
        .collect()
}

/// Steps of an operation; a concatenated operation is flattened one level.
fn steps(ctx: &Context, op: &Obj) -> Vec<Obj> {
    unsafe {
        if proj_get_type(op.0) != PJ_TYPE_PJ_TYPE_CONCATENATED_OPERATION {
            return vec![Obj(proj_clone(ctx.0, op.0))];
        }
        (0..proj_concatoperation_get_step_count(ctx.0, op.0)).filter_map(|i| Obj::new(proj_concatoperation_get_step(ctx.0, op.0, i))).collect()
    }
}

fn grids(ctx: &Context, op: &Obj) -> Vec<Value> {
    let count = unsafe { proj_coordoperation_get_grid_used_count(ctx.0, op.0) };
    (0..count)
        .filter_map(|i| {
            let (mut short, mut full, mut package, mut url) = (null(), null(), null(), null());
            let (mut direct, mut open, mut available) = (0, 0, 0);
            let ok = unsafe {
                proj_coordoperation_get_grid_used(ctx.0, op.0, i, &mut short, &mut full, &mut package, &mut url, &mut direct, &mut open, &mut available)
            };
            let name = text(short)?;
            (ok != 0).then(|| json!({"name": name, "url": format!("{GRID_CDN}{name}"), "openLicense": open != 0}))
        })
        .collect()
}

/// The datum part of a proj4 string for one candidate, or why there is none.
enum Datum {
    Same,
    Null,
    Helmert([f64; 7]),
    Grid(Vec<String>),
    Ballpark,
    Unsupported(String),
}

fn datum(ctx: &Context, op: &Obj, same_datum: bool) -> (Datum, Vec<Value>) {
    if unsafe { proj_coordoperation_has_ballpark_transformation(ctx.0, op.0) } != 0 {
        return (Datum::Ballpark, vec![]);
    }
    let mut helmert = Vec::new();
    let mut grid_names = Vec::new();
    let mut transformations = Vec::new();
    let mut unsupported = None;
    for step in steps(ctx, op) {
        if unsafe { proj_get_type(step.0) } == PJ_TYPE_PJ_TYPE_CONVERSION {
            continue;
        }
        let (mut method, mut auth, mut code) = (null(), null(), null());
        unsafe { proj_coordoperation_get_method_info(ctx.0, step.0, &mut method, &mut auth, &mut code) };
        let method = text(method).unwrap_or_default();
        transformations.push(json!({"name": name_of(&step), "id": id_of(&step), "method": method}));
        if text(code).as_deref() == Some(LONGITUDE_ROTATION) {
            continue;
        }
        // PROJ reports an inverted step through its forward parameters.
        let inverse = method.starts_with("Inverse of ");
        let step_grids = grids(ctx, &step);
        if !step_grids.is_empty() {
            // `+nadgrids=a,b` picks whichever grid covers the point; it cannot chain grids or run one backwards.
            if inverse {
                unsupported.get_or_insert("grid used in reverse".to_string());
            } else if !grid_names.is_empty() {
                unsupported.get_or_insert("two grid steps".to_string());
            }
            grid_names.extend(step_grids.iter().map(|g| g["name"].as_str().unwrap().to_string()));
            continue;
        }
        let mut values = [0.0; 7];
        if unsafe { proj_coordoperation_get_towgs84_values(ctx.0, step.0, values.as_mut_ptr(), 7, 0) } != 0 {
            if values.iter().any(|v| *v != 0.0) {
                helmert.push(values.map(|v| if inverse { -v } else { v }));
            }
        } else {
            unsupported.get_or_insert(format!("{method} has no proj4 form"));
        }
    }
    let kind = match (unsupported, helmert.len(), grid_names.is_empty()) {
        (Some(reason), _, _) => Datum::Unsupported(reason),
        (None, 0, true) if transformations.is_empty() && same_datum => Datum::Same,
        (None, 0, true) => Datum::Null,
        (None, 1, true) => Datum::Helmert(helmert[0]),
        (None, 0, false) => Datum::Grid(grid_names),
        (None, n, _) => Datum::Unsupported(format!("{n} Helmert steps and {} grids", grid_names.len())),
    };
    (kind, transformations)
}

fn format_number(v: f64) -> String {
    let v = if v == 0.0 { 0.0 } else { v };
    let s = format!("{}", (v * 1e12).round() / 1e12);
    if s == "-0" { "0".into() } else { s }
}

/// Inserts the datum token before `+units`, `+to_meter` or `+no_defs`.
fn with_token(base: &str, token: &str) -> String {
    let mut parts: Vec<&str> = base.split_whitespace().collect();
    let at = parts.iter().position(|t| t.starts_with("+units") || t.starts_with("+to_meter") || t.starts_with("+no_defs")).unwrap_or(parts.len());
    parts.insert(at, token);
    parts.join(" ")
}

/// The CRS's proj4 string without any datum shift, with `+datum` names proj4js would misread replaced by ellipsoids.
fn base_definition(crs_proj4: &str) -> Result<String, String> {
    let mut out = Vec::new();
    for t in crs_proj4.split_whitespace() {
        if t.starts_with("+towgs84=") || (t.starts_with("+nadgrids=") && t != "+nadgrids=@null") {
            continue;
        }
        match t.strip_prefix("+datum=") {
            Some("WGS84") | None => out.push(t.to_string()),
            Some("NAD83") => out.push("+ellps=GRS80".into()),
            Some("NAD27") => out.push("+ellps=clrk66".into()),
            Some(other) => return Err(format!("unexpected +datum={other}")),
        }
    }
    let out = out.join(" ");
    Ok(PROJ4JS_MISSING_ELLIPSOIDS.iter().fold(out, |def, (name, params)| def.replace(&format!("+ellps={name}"), params)))
}

fn build(ctx: &Context, wgs84: &Obj, authority: &str, kind: &str, code: &str) -> Value {
    let mut result = json!({"id": format!("{authority}:{code}"), "kind": kind});
    let Some(crs) = from_database(ctx, authority, code) else {
        result["error"] = "PROJ could not create the CRS".into();
        return result;
    };
    result["name"] = name_of(&crs).into();
    let body = text(unsafe { proj_get_celestial_body_name(ctx.0, crs.0) }).unwrap_or_default();
    if body != "Earth" {
        result["error"] = format!("not on Earth ({body})").into();
        return result;
    }
    let (mut w, mut s, mut e, mut n, mut area) = (0.0, 0.0, 0.0, 0.0, null());
    if unsafe { proj_get_area_of_use(ctx.0, crs.0, &mut w, &mut s, &mut e, &mut n, &mut area) } == 0 || w < -1000.0 {
        result["error"] = "no area of use".into();
        return result;
    }
    let bbox = [w, s, e, n];
    result["area"] = json!({"name": text(area), "bbox": bbox});

    let Some(crs_proj4) = proj4_of(ctx, &crs) else {
        result["error"] = "PROJ has no proj4 form for this CRS".into();
        return result;
    };
    let base = match base_definition(&crs_proj4) {
        Ok(base) => base,
        Err(reason) => {
            result["error"] = reason.into();
            return result;
        }
    };
    result["base"] = base.clone().into();
    let same_datum = match Obj::new(unsafe { proj_crs_get_geodetic_crs(ctx.0, crs.0) }) {
        Some(geodetic) => (unsafe { proj_is_equivalent_to(geodetic.0, wgs84.0, PJ_COMPARISON_CRITERION_PJ_COMP_EQUIVALENT_EXCEPT_AXIS_ORDER_GEOGCRS) }) != 0,
        None => false,
    };
    result["sameDatum"] = same_datum.into();

    // Reference positions: PROJ's best operation at each point, then the same position as a proj4 string reads it.
    let Some(base_crs) = create(ctx, &format!("{base} +type=crs")) else {
        result["error"] = "PROJ cannot read the base proj4 string".into();
        return result;
    };
    let (Some(best), Some(to_proj4)) = (transformer(ctx, wgs84, &crs), transformer(ctx, &crs, &base_crs)) else {
        result["error"] = "PROJ could not build the reference transformations".into();
        return result;
    };
    // Points where PROJ only had a ballpark operation say nothing about the datum, so they are marked.
    let mut ballpark = Vec::new();
    let points: Vec<([f64; 2], [f64; 2], [f64; 2])> = sample_points(bbox)
        .into_iter()
        .filter_map(|lonlat| {
            let native = apply(&best, lonlat[1], lonlat[0])?;
            let used = Obj::new(unsafe { proj_trans_get_last_used_operation(best.0) });
            let xy = apply(&to_proj4, native[0], native[1])?;
            ballpark.push(used.is_some_and(|op| unsafe { proj_coordoperation_has_ballpark_transformation(ctx.0, op.0) } != 0));
            Some((lonlat, native, xy))
        })
        .collect();
    if points.is_empty() {
        result["error"] = "PROJ could not transform any sample point".into();
        return result;
    }
    result["points"] = points.iter().zip(&ballpark).map(|((lonlat, _, xy), ballpark)| json!({"lonlat": lonlat, "xy": xy, "ballpark": ballpark})).collect();

    let factory = unsafe { proj_create_operation_factory_context(ctx.0, null()) };
    unsafe {
        proj_operation_factory_context_set_spatial_criterion(ctx.0, factory, PROJ_SPATIAL_CRITERION_PROJ_SPATIAL_CRITERION_PARTIAL_INTERSECTION);
        proj_operation_factory_context_set_grid_availability_use(ctx.0, factory, PROJ_GRID_AVAILABILITY_USE_PROJ_GRID_AVAILABILITY_IGNORED);
        proj_operation_factory_context_set_area_of_interest(ctx.0, factory, w, s, e, n);
        proj_operation_factory_context_set_allow_ballpark_transformations(ctx.0, factory, 1);
    }
    let list = unsafe { proj_create_operations(ctx.0, crs.0, wgs84.0, factory) };
    let count = if list.is_null() { 0 } else { unsafe { proj_list_get_count(list) } };
    let mut candidates = Vec::new();
    for i in 0..count.min(MAX_CANDIDATES) {
        let Some(op) = Obj::new(unsafe { proj_list_get(ctx.0, list, i) }) else { continue };
        let accuracy = unsafe { proj_coordoperation_get_accuracy(ctx.0, op.0) };
        let (datum, transformations) = datum(ctx, &op, same_datum);
        let mut candidate = json!({
            "name": name_of(&op),
            "accuracy": (accuracy >= 0.0).then_some(accuracy),
            "transformations": transformations,
        });
        let token = match &datum {
            Datum::Same | Datum::Ballpark => None,
            Datum::Null => Some("+towgs84=0,0,0,0,0,0,0".to_string()),
            Datum::Helmert(v) => {
                let values = if v[3..].iter().all(|x| *x == 0.0) { &v[..3] } else { &v[..] };
                Some(format!("+towgs84={}", values.iter().map(|x| format_number(*x)).collect::<Vec<_>>().join(",")))
            }
            Datum::Grid(names) => Some(format!("+nadgrids={}", names.join(","))),
            Datum::Unsupported(_) => None,
        };
        candidate["kind"] = match &datum {
            Datum::Same => "same-datum",
            Datum::Null => "null",
            Datum::Helmert(_) => "helmert",
            Datum::Grid(_) => "grid",
            Datum::Ballpark => "ballpark",
            Datum::Unsupported(_) => "unsupported",
        }
        .into();
        if let Datum::Unsupported(reason) = &datum {
            candidate["reason"] = reason.clone().into();
            candidates.push(candidate);
            continue;
        }
        if let Datum::Grid(_) = &datum {
            candidate["grids"] = grids(ctx, &op).into();
        }
        let definition = match &token {
            Some(token) => with_token(&base, token),
            None => base.clone(),
        };
        candidate["proj4"] = definition.clone().into();

        // How PROJ reads the proj4 string, compared with the operation it encodes.
        if let Some(as_string) = create(ctx, &format!("{definition} +type=crs")).and_then(|crs4| transformer(ctx, &crs4, wgs84)) {
            let mut fidelity: f64 = 0.0;
            let mut compared = 0;
            let mut via_proj = Vec::new();
            for (_, native, xy) in &points {
                let read = apply_to_lonlat(&as_string, xy[0], xy[1]);
                if let (Some(read), Some(expected)) = (read, apply_to_lonlat(&op, native[0], native[1])) {
                    fidelity = fidelity.max(distance(read, expected));
                    compared += 1;
                }
                via_proj.push(read.map(|r| json!(r)).unwrap_or(Value::Null));
            }
            candidate["fidelityM"] = (compared > 0).then_some(fidelity).into();
            candidate["fidelityPoints"] = compared.into();
            candidate["viaProj"] = via_proj.into();
        }
        candidates.push(candidate);
        if matches!(datum, Datum::Same) {
            break;
        }
    }
    unsafe {
        proj_list_destroy(list);
        proj_operation_factory_context_destroy(factory);
    }
    // PROJ can offer no usable operation for a CRS already on WGS 84: EPSG:4326 to itself is a zero offset step.
    if same_datum && !candidates.iter().any(|c| c["kind"] == "same-datum") {
        if let Some(as_string) = create(ctx, &format!("{base} +type=crs")).and_then(|crs4| transformer(ctx, &crs4, wgs84)) {
            let read: Vec<Option<[f64; 2]>> = points.iter().map(|(_, _, xy)| apply_to_lonlat(&as_string, xy[0], xy[1])).collect();
            let fidelity = points.iter().zip(&read).filter_map(|((lonlat, _, _), r)| r.map(|r| distance(r, *lonlat))).fold(0.0, f64::max);
            candidates.push(json!({
                "name": "same datum",
                "accuracy": 0.0,
                "transformations": [],
                "kind": "same-datum",
                "proj4": base,
                "fidelityM": fidelity,
                "fidelityPoints": read.iter().flatten().count(),
                "viaProj": read.iter().map(|r| r.map(|r| json!(r)).unwrap_or(Value::Null)).collect::<Vec<_>>(),
            }));
        }
    }
    result["candidates"] = candidates.into();
    result
}

fn main() {
    let out_dir = PathBuf::from(std::env::args().nth(1).unwrap_or_else(|| "../data/raw".into()));
    fs::create_dir_all(&out_dir).unwrap();
    let ctx = Context::new();
    let only: Option<Vec<String>> = std::env::var("CODES").ok().map(|v| v.split(',').map(str::to_string).collect());
    let mut work: Vec<(&str, &str, String)> = Vec::new();
    for authority in AUTHORITIES {
        fs::create_dir_all(out_dir.join(authority)).unwrap();
        for (kind, ty) in [("projected", PJ_TYPE_PJ_TYPE_PROJECTED_CRS), ("geographic", PJ_TYPE_PJ_TYPE_GEOGRAPHIC_2D_CRS)] {
            for code in codes(&ctx, authority, ty) {
                if only.as_ref().is_none_or(|o| o.contains(&format!("{authority}:{code}"))) {
                    work.push((authority, kind, code));
                }
            }
        }
    }
    let meta = |key: &str| text(unsafe { proj_context_get_database_metadata(ctx.0, c(key).as_ptr()) });
    let versions = json!({
        "proj": text(unsafe { proj_info() }.release),
        "epsg": meta("EPSG.VERSION"),
        "epsgDate": meta("EPSG.DATE"),
        "esri": meta("ESRI.VERSION"),
    });
    fs::write(out_dir.join("../versions.json"), serde_json::to_string_pretty(&versions).unwrap()).unwrap();
    eprintln!("{} CRSs, {}", work.len(), versions);

    let next = AtomicUsize::new(0);
    let done = Mutex::new(0usize);
    let threads = std::env::var("THREADS").ok().and_then(|v| v.parse().ok()).unwrap_or_else(|| std::thread::available_parallelism().map(|n| n.get()).unwrap_or(8));
    std::thread::scope(|scope| {
        for _ in 0..threads {
            scope.spawn(|| {
                let ctx = Context::new();
                let wgs84 = from_database(&ctx, "EPSG", "4326").unwrap();
                loop {
                    let i = next.fetch_add(1, Ordering::Relaxed);
                    let Some((authority, kind, code)) = work.get(i) else { break };
                    let result = build(&ctx, &wgs84, authority, kind, code);
                    fs::write(out_dir.join(authority).join(format!("{code}.json")), serde_json::to_string(&result).unwrap()).unwrap();
                    let mut done = done.lock().unwrap();
                    *done += 1;
                    if *done % 250 == 0 {
                        eprintln!("{}/{}", *done, work.len());
                    }
                }
            });
        }
    });
}
