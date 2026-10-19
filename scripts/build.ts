// Picks one definition per CRS from the generator's candidates, checks it in proj4js and writes the package:
// index.js with a constant per CRS, <AUTH>/<code>.js and <AUTH>/<code>.json for each CRS, all.json and catalog.json.
import {mkdir, readdir, readFile, rm, writeFile} from 'node:fs/promises';
import {createRequire} from 'node:module';

const require = createRequire(import.meta.url);
// PROJ4JS points at another proj4js checkout.
const proj4Path = process.env.PROJ4JS ?? 'proj4';
const proj4 = require(proj4Path);
const proj4Version: string = require(`${proj4Path}/package.json`).version;

const ROOT = new URL('../', import.meta.url).pathname;
const RAW = `${ROOT}data/raw/`;
const AUTHORITIES = ['EPSG', 'ESRI'];
// Candidates within this of the best one count as equally good, so PROJ's own order decides.
const TIE_M = 0.01;
// A definition further than this from PROJ's best operation is an approximation, not the registry's own numbers.
const EXACT_M = 0.01;

type LonLat = [number, number];
type Candidate = {
  name: string;
  kind: 'same-datum' | 'null' | 'helmert' | 'grid' | 'ballpark' | 'unsupported';
  accuracy: number | null;
  proj4?: string;
  reason?: string;
  grids?: {name: string; url: string; openLicense: boolean}[];
  transformations: {name: string; id: string | null; method: string}[];
  viaProj?: (LonLat | null)[];
};
type Raw = {
  id: string;
  kind: 'projected' | 'geographic';
  name: string;
  area?: {name: string; bbox: number[]};
  error?: string;
  deprecated?: boolean;
  replacedBy?: string[];
  points?: {lonlat: LonLat; xy: [number, number]; ballpark: boolean}[];
  candidates?: Candidate[];
};

function distance([lon1, lat1]: number[], [lon2, lat2]: number[]): number {
  const r = Math.PI / 180;
  const a = Math.sin((lat2 - lat1) * r / 2) ** 2 + Math.cos(lat1 * r) * Math.cos(lat2 * r) * Math.sin((lon2 - lon1) * r / 2) ** 2;
  return 2 * 6371008.8 * Math.asin(Math.min(1, Math.sqrt(a)));
}

const max = (values: number[]) => (values.length ? Math.max(...values) : null);
const round = (v: number | null) => (v === null ? null : v < 0.001 ? 0 : Number(v.toPrecision(3)));

// The sample points to score on: where PROJ had only a ballpark operation, it knows nothing about the datum.
function scoredPoints(raw: Raw) {
  const points = raw.points!.map((p, i) => ({...p, i}));
  const known = points.filter(p => !p.ballpark);
  return known.length > 0 ? known : points;
}

// Error of the definition as PROJ reads it, against PROJ's best operation at each point.
function datumErrors(raw: Raw, candidate: Candidate): number[] {
  return scoredPoints(raw).flatMap(p => {
    const read = candidate.viaProj?.[p.i];
    return read ? [distance(read, p.lonlat)] : [];
  });
}

// Error of the definition as proj4js reads it, against PROJ's best operation, or null if proj4js can't use it.
function proj4jsError(raw: Raw, definition: string): number | null {
  let converter;
  try {
    converter = proj4(definition);
  } catch {
    return null;
  }
  // proj4js ignores +axis unless the caller passes enforceAxis.
  const enforceAxis = /\+axis=/.test(definition);
  const errors: number[] = [];
  for (const p of scoredPoints(raw)) {
    let out;
    try {
      out = converter.inverse([...p.xy], enforceAxis);
    } catch {
      return null;
    }
    if (!out || !Number.isFinite(out[0]) || !Number.isFinite(out[1])) return null;
    errors.push(distance(out, p.lonlat));
  }
  return round(max(errors));
}

const DATUM_SHIFT = {'same-datum': 'none', null: 'null', helmert: 'helmert', ballpark: 'unknown'} as Record<string, string>;

// Named after its datum steps, without the projection PROJ wraps around them.
function transformationOf(candidate: Candidate) {
  return {
    name: candidate.transformations.map(t => t.name).join(' + ') || candidate.name,
    accuracyM: candidate.accuracy,
    steps: candidate.transformations.map(({id, name, method}) => ({id, name, method}))
  };
}

function evaluate(raw: Raw) {
  if (raw.error) return {id: raw.id, failed: raw.error};
  const candidates = raw.candidates ?? [];
  const scored = candidates
    .map((candidate, order) => ({candidate, order, errors: datumErrors(raw, candidate)}))
    .filter(s => s.candidate.proj4 && s.candidate.kind !== 'grid' && s.errors.length > 0);
  if (scored.length === 0) return {id: raw.id, failed: candidates.find(c => c.reason)?.reason ?? 'no candidate without grids'};
  const best = Math.min(...scored.map(s => max(s.errors)!));
  const {candidate, errors} = scored.filter(s => max(s.errors)! <= best + TIE_M).sort((a, b) => a.order - b.order)[0];
  const definition = candidate.proj4!;
  const errorM = round(max(errors))!;
  const grid = candidates.find(c => c.kind === 'grid' && c.proj4);
  const gridErrorM = grid ? round(max(datumErrors(raw, grid))) : null;
  return {
    id: raw.id,
    name: raw.name,
    type: raw.kind,
    area: raw.area!,
    definition,
    datumShift: DATUM_SHIFT[candidate.kind],
    transformation: candidate.kind === 'helmert' || candidate.kind === 'null' ? transformationOf(candidate) : null,
    errorM,
    proj4jsErrorM: proj4jsError(raw, definition),
    approximate: errorM > EXACT_M,
    ...(raw.deprecated && {deprecated: true, replacedBy: raw.replacedBy ?? []}),
    // Only offered when the grid gets closer to PROJ than the definition above.
    ...(grid && gridErrorM !== null && gridErrorM < errorM && {
      grid: {
        definition: grid.proj4!,
        files: grid.grids!.map(g => g.url),
        transformation: transformationOf(grid),
        errorM: gridErrorM
      }
    })
  };
}

type Result = ReturnType<typeof evaluate>;
type Definition = Exclude<Result, {failed: string}>;

function moduleSource(r: Definition): string {
  return [
    `export const id = ${JSON.stringify(r.id)};`,
    `export const name = ${JSON.stringify(r.name)};`,
    `export const bbox = ${JSON.stringify(r.area.bbox)};`,
    `export const definition = ${JSON.stringify(r.definition)};`,
    `export const errorM = ${JSON.stringify(r.errorM)};`,
    ''
  ].join('\n');
}

const versions = JSON.parse(await readFile(`${ROOT}data/versions.json`, 'utf8'));
const meta = {...versions, proj4js: proj4Version};
const definitions: Definition[] = [];
const failed: {id: string; failed: string}[] = [];
for (const authority of AUTHORITIES) {
  const files = (await readdir(`${RAW}${authority}`)).filter(f => f.endsWith('.json'));
  const results = await Promise.all(files.map(async f => evaluate(JSON.parse(await readFile(`${RAW}${authority}/${f}`, 'utf8')))));
  const ok = results.filter((r): r is Definition => !('failed' in r)).sort((a, b) => Number(a.id.split(':')[1]) - Number(b.id.split(':')[1]));
  failed.push(...results.filter((r): r is {id: string; failed: string} => 'failed' in r));
  await rm(`${ROOT}${authority}`, {recursive: true, force: true});
  await mkdir(`${ROOT}${authority}`, {recursive: true});
  for (const r of ok) {
    const code = r.id.split(':')[1];
    await writeFile(`${ROOT}${authority}/${code}.js`, moduleSource(r));
    await writeFile(`${ROOT}${authority}/${code}.json`, JSON.stringify(r) + '\n');
  }
  definitions.push(...ok);
}

const count = (f: (r: Definition) => boolean) => definitions.filter(f).length;

// The package root: one string constant per CRS, named like EPSG_28992, so editors can auto-import them.
const constant = (r: Definition) => r.id.replace(':', '_');
const comment = (r: Definition) => {
  const accuracy = r.errorM <= EXACT_M ? 'Within 1 cm of PROJ.' : `Up to ${r.errorM} m from PROJ.`;
  const proj4js = r.proj4jsErrorM === null ? ` proj4js ${proj4Version} can't read it.` : '';
  const shift = r.datumShift === 'unknown' ? ' The registry has no transformation to WGS 84.' : '';
  const replacement = r.replacedBy?.length ? `Use ${r.replacedBy.join(' or ')}.` : 'The registry deprecated this code.';
  const deprecated = r.deprecated ? `\n *\n * @deprecated ${replacement}` : '';
  const text = `${r.id}, ${r.name}. ${r.area.name} ${accuracy}${shift}${proj4js}\n *\n * \`${r.definition}\`${deprecated}`;
  return `/**\n * ${text.replaceAll('*/', '* /')}\n */`;
};
await writeFile(`${ROOT}index.js`, definitions.map(r => `export const ${constant(r)} = ${JSON.stringify(r.definition)};`).join('\n') + '\n');
await writeFile(`${ROOT}index.d.ts`, definitions.map(r => `${comment(r)}\nexport declare const ${constant(r)}: string;`).join('\n') + '\n');

await writeFile(`${ROOT}all.json`, JSON.stringify(Object.fromEntries(definitions.map(r => [r.id, r.definition]))) + '\n');
await writeFile(`${ROOT}catalog.json`, JSON.stringify({
  versions: meta,
  crs: definitions.map(r => ({id: r.id, name: r.name, type: r.type, bbox: r.area.bbox, datumShift: r.datumShift, errorM: r.errorM, approximate: r.approximate, ...(r.deprecated && {deprecated: true})}))
}) + '\n');
await mkdir(`${ROOT}data`, {recursive: true});
await writeFile(`${ROOT}data/failed.json`, JSON.stringify(failed.sort((a, b) => a.id.localeCompare(b.id)), null, 1) + '\n');

console.log(meta);
console.log(`${definitions.length} definitions (${count(r => !!r.deprecated)} deprecated), ${failed.length} without one`);
console.log(`within 1 cm of PROJ: ${count(r => r.errorM <= 0.01)}, within 1 m: ${count(r => r.errorM <= 1)}`);
console.log(`in proj4js within 1 m: ${count(r => r.proj4jsErrorM !== null && r.proj4jsErrorM <= 1)}, proj4js can't read: ${count(r => r.proj4jsErrorM === null)}`);
console.log(`grid closer: ${count(r => 'grid' in r)}`);
