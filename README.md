# crs-defs

PROJ strings for 7,753 EPSG and ESRI codes, plus 1,755 deprecated ones. Each one carries the datum shift to WGS 84 that PROJ itself would use, and a measurement of how far it lands from PROJ.

Most web maps copy their CRS definitions from a lookup site and hand them to proj4js. For grids on an older datum, the shift to WGS 84 is often missing or has its rotations the wrong way round, and data lands tens to hundreds of meters off. Every definition here is written by PROJ and checked against PROJ's best transformation at 17 points across the CRS's area of use, both as PROJ reads it and as proj4js reads it.

## Install

```sh
npm install crs-defs
```

## Use

Every code is a string constant named after it, `EPSG_28992` or `ESRI_54009`. Editors offer the import as you type the name and show the CRS's name, area and accuracy on hover. Bundlers keep only the constants you import.

```js
import proj4 from 'proj4';
import {EPSG_28992} from 'crs-defs';

proj4.defs('EPSG:28992', EPSG_28992);
proj4('EPSG:28992', [4.8932, 52.3731]); // [121358.61, 487361.81]
```

In MapLibre GL JS 6.15 and later:

```js
import * as maplibregl from 'maplibre-gl';
import proj4 from 'proj4';
import {EPSG_4326, EPSG_28992} from 'crs-defs';

maplibregl.addProjection({
  name: 'EPSG:28992',
  projection: proj4(EPSG_4326, EPSG_28992),
  definition: EPSG_28992,
  tileMatrixSet: {origin: [-285401.92, 903401.92], extentAtZoom0: 256 * 3440.64}
});
```

MapLibre registers one CRS per name and always works in longitude and latitude. `projection` is the pair of functions between the two, `forward` from lng/lat to the grid's meters and `inverse` back, and proj4 builds that pair from WGS 84 and the grid's definition. `definition` is the string MapLibre's workers use to place GeoJSON, together with the projection handler from MapLibre's custom CRS guide.

A development server loads the whole module (1.5 MB). `crs-defs/EPSG/28992` holds just one code, with its name, area of use and error as separate exports:

```js
import {definition, bbox, errorM} from 'crs-defs/EPSG/28992';
```

In OpenLayers, looking codes up when they're needed:

```js
import proj4 from 'proj4';
import {fromProjectionCode, register, setProjectionCodeLookup} from 'ol/proj/proj4.js';

register(proj4);
setProjectionCodeLookup(async code => {
  const response = await fetch(`https://cdn.jsdelivr.net/npm/crs-defs@0/${code.replace(':', '/')}.json`);
  return (await response.json()).definition;
});
const projection = await fromProjectionCode('EPSG:28992');
```

Every definition at once, for a CRS picker, is 1.3 MB (132 kB gzipped):

```js
import all from 'crs-defs/all.json' with {type: 'json'};

proj4.defs(Object.entries(all));
```

`catalog.json` lists every code with its name, area of use and accuracy, without the strings.

## Records

Each code also has a JSON record, `crs-defs/EPSG/28992.json`, which jsDelivr serves at `https://cdn.jsdelivr.net/npm/crs-defs@0/EPSG/28992.json`.

```json
{
  "id": "EPSG:28992",
  "name": "RD",
  "type": "projected",
  "area": {"bbox": [2.53, 50.75, 7.22, 55.77], "name": "Netherlands - onshore and offshore."},
  "definition": "+proj=sterea +lat_0=52.1561605555556 +lon_0=5.38763888888889 +k=0.9999079 +x_0=155000 +y_0=463000 +ellps=bessel +towgs84=565.4171,50.3319,465.5524,-0.398957388243,0.343987817378,-1.87740163998,4.0725 +units=m +no_defs",
  "datumShift": "helmert",
  "transformation": {
    "name": "Amersfoort to WGS 84 (4)",
    "accuracyM": 1.5,
    "steps": [{"id": "EPSG:4833", "name": "Amersfoort to WGS 84 (4)", "method": "Coordinate Frame rotation (geog2D domain)"}]
  },
  "errorM": 0,
  "proj4jsErrorM": 0,
  "approximate": false
}
```

| Field | Meaning |
|---|---|
| `definition` | The PROJ string, with the datum shift as `+towgs84`. |
| `datumShift` | `helmert` for a 3 or 7 parameter shift, `null` when the registry treats the datum as WGS 84, `none` when it is WGS 84, and `unknown` when the registry has no transformation to WGS 84. |
| `transformation` | The registry transformation the shift comes from, with its stated accuracy. |
| `errorM` | The largest distance in meters from PROJ's best transformation, over 17 points in the area of use. |
| `proj4jsErrorM` | The same in proj4js 2.22.0, or `null` when proj4js can't use the string. |
| `approximate` | `true` when `errorM` is more than 1 cm. |
| `grid` | Only when a grid file gets closer. A `+nadgrids` definition, its files on cdn.proj.org and its `errorM`. |
| `deprecated`, `replacedBy` | Only on deprecated codes. `true`, and the codes the registry names as replacements, if any. |

Deprecated codes such as ESRI:102100, the Web Mercator ID ArcGIS servers still report, keep working. Their constants are marked `@deprecated` with the replacement, so editors strike them through and say "Use EPSG:3857". A code the registry deprecates later stays in the package, marked the same way, so an import never breaks.

## How close

| | EPSG | ESRI | Deprecated, both |
|---|---|---|---|
| Definitions | 6,525 | 1,228 | 1,755 |
| Within 1 cm of PROJ | 5,575 | 1,006 | 1,444 |
| Within 1 m of PROJ | 5,918 | 1,067 | 1,578 |

Where the official transformation is a grid file, a single `+towgs84` can't match it everywhere. 1,027 records offer a `grid` definition that gets closer. cdn.proj.org allows cross-origin requests, so proj4js can load the grids with [geotiff](https://www.npmjs.com/package/geotiff):

```js
import {fromUrl} from 'geotiff';

const record = await (await fetch('https://cdn.jsdelivr.net/npm/crs-defs@0/EPSG/27700.json')).json();
for (const url of record.grid.files) await proj4.nadgrid(url.split('/').pop(), await fromUrl(url)).ready;
proj4.defs('EPSG:27700', record.grid.definition);
```

For the British National Grid in London, that takes proj4js from 1.7 m to under 1 mm from PROJ.

Not covered:

- 141 CRSs have no definition, because PROJ can't write them as a PROJ string or their transformation has no PROJ string form. `data/failed.json` in the repository lists them.
- 133 definitions use projections proj4js 2.22 doesn't have, such as Winkel Tripel, Eckert I to V, Natural Earth and the Colombian urban grids. They're correct PROJ strings.
- ESRI codes for other planets are left out.

## Checked against national services

Agreeing with PROJ says little on its own, since PROJ writes the definitions. These checks run them in proj4js against each country's official transformation, which doesn't go through PROJ. The official inputs are ETRS89 coordinates, fed in as lon/lat the way web data arrives, at height 0 as in a 2D map.

| Grid | Reference | Points | Definition, median / max | With its `grid`, median / max |
|---|---|---|---|---|
| RD, EPSG:28992 | NSGI's RDNAPTRANS™2018 validation set | 7,959 in the Netherlands | 3 cm / 24 cm | |
| British National Grid, EPSG:27700 | Ordnance Survey's OSTN15 test points | 40 | 1.8 m / 4.9 m | 1 mm / 8 mm |
| CH1903+ / LV95, EPSG:2056 | swisstopo's REFRAME service | 30 places | under 1 mm / under 1 mm | |

LV95 is defined by a 3-parameter shift from ETRS89, so a PROJ string can match it exactly. RDNAPTRANS adds a correction grid to its datum shift, which a single `+towgs84` can't carry. OSTN15 is a grid, so only the `grid` definition matches it.

`npm run check:national` downloads the reference data and writes `data/national-checks.json`.

## Versions

This release is built from EPSG v13.105 (2026-10-12) and Esri's ArcGIS Pro 3.6 data, through PROJ master (9.10.0-dev, 83f5dd62), and checked with proj4js 2.22.0. `catalog.json` carries the versions.

## Terms

The EPSG data is owned by IOGP and used under the EPSG Dataset Terms of Use. Definitions marked `approximate` are not EPSG's own definitions. The ESRI data is copyright Esri, under the Apache License 2.0. The generator and scripts are MIT. See [NOTICE.md](NOTICE.md) and [LICENSE.md](LICENSE.md).

## Building

It needs a PROJ build with network support, Rust, and Node 22.18 or later.

```sh
cd generator
PKG_CONFIG_PATH=$PROJ/lib/pkgconfig cargo build --release
cd ..
DYLD_LIBRARY_PATH=$PROJ/lib PROJ_DATA=$PROJ/share/proj ./generator/target/release/crs-defs-generator data/raw
npm run build
npm test
```
