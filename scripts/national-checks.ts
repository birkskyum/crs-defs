// Checks the definitions in proj4js against national transformation services that don't go through PROJ.
// Downloads the reference data into data/national/ (not committed) and writes data/national-checks.json.
import {execFileSync} from 'node:child_process';
import {existsSync, mkdirSync, readFileSync, writeFileSync} from 'node:fs';
import {createRequire} from 'node:module';
import {fromArrayBuffer} from 'geotiff';

const require = createRequire(import.meta.url);
const proj4 = require('proj4');
const ROOT = new URL('../', import.meta.url).pathname;
const CACHE = `${ROOT}data/national/`;
mkdirSync(CACHE, {recursive: true});

type Point = {name: string; lonlat: [number, number]; xy: [number, number]};

async function download(file: string, url: string): Promise<Buffer> {
  const path = CACHE + file;
  if (!existsSync(path)) {
    const response = await fetch(url);
    if (!response.ok) throw new Error(`${url}: ${response.status}`);
    writeFileSync(path, Buffer.from(await response.arrayBuffer()));
  }
  return readFileSync(path);
}

// NSGI's RDNAPTRANS2018 self-validation set: ETRS89 and RD for the same points. NAP is `*` outside the Netherlands.
const Z001 = 'https://raw.githubusercontent.com/GeodetischeInfrastructuur/transformations/main/validate/data/Z001_ETRS89andRDNAP.txt';
async function dutch(): Promise<Point[]> {
  const text = (await download('Z001_ETRS89andRDNAP.txt', Z001)).toString('utf8');
  return text.trim().split('\n').map(line => line.trim().split(/\s+/)).filter(f => f[6] !== '*').map(f => ({
    name: f[0],
    lonlat: [Number(f[2]), Number(f[1])],
    xy: [Number(f[4]), Number(f[5])]
  }));
}

// Ordnance Survey's OSTN15 test points, from its developer pack.
const OS_PACK = 'https://www.ordnancesurvey.co.uk/documents/resources/OSTN15-OSGM15-DevelopersPack.zip';
async function british(): Promise<Point[]> {
  await download('OSTN15-OSGM15-DevelopersPack.zip', OS_PACK);
  const dir = `${CACHE}os/`;
  if (!existsSync(dir)) execFileSync('unzip', ['-q', '-o', `${CACHE}OSTN15-OSGM15-DevelopersPack.zip`, '-d', dir]);
  const rows = (file: string) => readFileSync(dir + file, 'utf8').trim().split(/\r?\n/).slice(1).map(l => l.split(','));
  const input = new Map(rows('OSTN15_OSGM15_TestInput_ETRStoOSGB.txt').map(f => [f[0], [Number(f[2]), Number(f[1])] as [number, number]]));
  return rows('OSTN15_OSGM15_TestOutput_ETRStoOSGB.txt').filter(f => Number(f[4]) !== 0).map(f => ({
    name: f[0],
    lonlat: input.get(f[0])!,
    xy: [Number(f[1]), Number(f[2])]
  }));
}

// swisstopo's REFRAME service for places across Switzerland and Liechtenstein, at height 0 like a 2D map in proj4js
// (at 500 m the answer moves about 1.3 cm). Answers are kept in the cache.
const SWISS_PLACES: [string, number, number][] = [
  ['Zürich', 8.5417, 47.3769], ['Geneva', 6.1432, 46.2044], ['Basel', 7.5886, 47.5596], ['Bern', 7.4474, 46.948],
  ['Lausanne', 6.6323, 46.5197], ['Lugano', 8.9511, 46.0037], ['Lucerne', 8.3093, 47.0502], ['St. Gallen', 9.3767, 47.4245],
  ['Chur', 9.532, 46.8508], ['Sion', 7.3606, 46.2331], ['Neuchâtel', 6.9293, 46.99], ['Schaffhausen', 8.635, 47.696],
  ['Fribourg', 7.161, 46.8065], ['Zermatt', 7.7491, 46.0207], ['St. Moritz', 9.8355, 46.4908], ['Bellinzona', 9.0244, 46.1946],
  ['Delémont', 7.3445, 47.3649], ['Scuol', 10.2988, 46.7963], ['Andermatt', 8.5944, 46.6356], ['Interlaken', 7.8632, 46.6863],
  ['Aarau', 8.0444, 47.3925], ['Frauenfeld', 8.8988, 47.5536], ['Glarus', 9.0679, 47.0404], ['Martigny', 7.0719, 46.1028],
  ['Davos', 9.836, 46.8027], ['Locarno', 8.7943, 46.1709], ['Thun', 7.628, 46.758], ['Winterthur', 8.7241, 47.4988],
  ['Brig', 7.9878, 46.3159], ['Vaduz', 9.5209, 47.141]
];
async function swiss(): Promise<Point[]> {
  const path = `${CACHE}reframe-h0.json`;
  const answers: Record<string, [number, number]> = existsSync(path) ? JSON.parse(readFileSync(path, 'utf8')) : {};
  for (const [name, lon, lat] of SWISS_PLACES) {
    if (answers[name]) continue;
    const response = await fetch(`https://geodesy.geo.admin.ch/reframe/wgs84tolv95?easting=${lon}&northing=${lat}&altitude=0&format=json`);
    const {easting, northing} = await response.json();
    answers[name] = [Number(easting), Number(northing)];
    await new Promise(resolve => setTimeout(resolve, 300));
  }
  writeFileSync(path, JSON.stringify(answers, null, 1));
  return SWISS_PLACES.map(([name, lon, lat]) => ({name, lonlat: [lon, lat], xy: answers[name]}));
}

async function loadGrid(url: string) {
  const name = url.split('/').pop()!;
  const data = await download(name, url);
  await proj4.nadgrid(name, await fromArrayBuffer(data.buffer.slice(data.byteOffset, data.byteOffset + data.byteLength))).ready;
}

function stats(definition: string, points: Point[]) {
  const converter = proj4('+proj=longlat +datum=WGS84 +no_defs', definition);
  const errors = points.map(p => {
    const [x, y] = converter.forward(p.lonlat);
    return Math.hypot(x - p.xy[0], y - p.xy[1]);
  }).sort((a, b) => a - b);
  const at = (q: number) => Number(errors[Math.min(errors.length - 1, Math.floor(q * errors.length))].toFixed(3));
  return {points: errors.length, medianM: at(0.5), p95M: at(0.95), maxM: Number(errors[errors.length - 1].toFixed(3))};
}

const checks = [
  {id: 'EPSG:28992', source: 'NSGI RDNAPTRANS2018 validation set (Z001), points in the Netherlands', url: Z001, points: await dutch()},
  {id: 'EPSG:27700', source: 'Ordnance Survey OSTN15 test points', url: OS_PACK, points: await british()},
  {id: 'EPSG:2056', source: 'swisstopo REFRAME, wgs84tolv95', url: 'https://geodesy.geo.admin.ch/reframe/', points: await swiss()}
];

const results = [];
for (const check of checks) {
  const record = JSON.parse(readFileSync(`${ROOT}${check.id.replace(':', '/')}.json`, 'utf8'));
  const result: Record<string, unknown> = {id: check.id, name: record.name, source: check.source, url: check.url, definition: stats(record.definition, check.points)};
  if (record.grid) {
    for (const url of record.grid.files) await loadGrid(url);
    result.grid = stats(record.grid.definition, check.points);
  }
  results.push(result);
}

const pkg = JSON.parse(readFileSync(`${ROOT}package.json`, 'utf8'));
const proj4Version = require('proj4/package.json').version;
writeFileSync(`${ROOT}data/national-checks.json`, JSON.stringify({crsDefs: pkg.version, proj4js: proj4Version, checks: results}, null, 1) + '\n');
for (const r of results) console.log(JSON.stringify(r));
