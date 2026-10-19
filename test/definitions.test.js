import assert from 'node:assert/strict';
import {readdir, readFile} from 'node:fs/promises';
import {test} from 'node:test';
import proj4 from 'proj4';
import * as constants from 'crs-defs';

const root = new URL('../', import.meta.url);
const readJson = async path => JSON.parse(await readFile(new URL(path, root), 'utf8'));

// Positions from PROJ 9.10 (EPSG v13.105) with its grids, as cs2cs gives them for these lon/lat.
const places = [
  {id: 'EPSG:28992', lonlat: [4.8932, 52.3731], xy: [121358.613, 487361.8119], toleranceM: 0.05},
  {id: 'EPSG:31370', lonlat: [4.3517, 50.8503], xy: [148799.1702, 171100.1548], toleranceM: 0.05},
  {id: 'EPSG:2056', lonlat: [7.4474, 46.948], xy: [2600667.4753, 1199657.3164], toleranceM: 0.05},
  {id: 'EPSG:27700', lonlat: [-0.1276, 51.5034], xy: [530052.2698, 179935.4447], toleranceM: 5},
  {id: 'EPSG:2154', lonlat: [2.3522, 48.8566], xy: [652469.0227, 6862035.2594], toleranceM: 0.05},
  {id: 'EPSG:5514', lonlat: [14.4208, 50.0875], xy: [-742841.2809, -1043004.6231], toleranceM: 2},
  {id: 'EPSG:3857', lonlat: [12.5683, 55.6761], xy: [1399096.7561, 7494204.6909], toleranceM: 0.05},
  {id: 'ESRI:54009', lonlat: [12.5683, 55.6761], xy: [879932.4922, 6454269.3871], toleranceM: 0.05},
  {id: 'ESRI:102003', lonlat: [-98, 38], xy: [-173949.7719, 57869.2491], toleranceM: 1}
];

function distance([lon1, lat1], [lon2, lat2]) {
  const r = Math.PI / 180;
  const a = Math.sin((lat2 - lat1) * r / 2) ** 2 + Math.cos(lat1 * r) * Math.cos(lat2 * r) * Math.sin((lon2 - lon1) * r / 2) ** 2;
  return 2 * 6371008.8 * Math.asin(Math.min(1, Math.sqrt(a)));
}

for (const {id, lonlat, xy, toleranceM} of places) {
  test(`${id} in proj4js lands where PROJ puts it`, async () => {
    const definition = constants[id.replace(':', '_')];
    const errorM = distance(proj4(definition).inverse(xy), lonlat);
    assert.ok(errorM <= toleranceM, `${errorM.toFixed(3)} m off`);
  });
}

test('EPSG:4326 is plain WGS 84', async () => {
  assert.equal(constants.EPSG_4326, '+proj=longlat +datum=WGS84 +no_defs');
});

test('constants, modules, records, all.json and catalog.json list the same definitions', async () => {
  const all = await readJson('all.json');
  const {crs} = await readJson('catalog.json');
  const files = [];
  for (const authority of ['EPSG', 'ESRI']) {
    for (const f of await readdir(new URL(`${authority}/`, root))) if (f.endsWith('.js')) files.push(`${authority}:${f.slice(0, -3)}`);
  }
  assert.deepEqual(new Set(files), new Set(Object.keys(all)));
  assert.deepEqual(new Set(crs.map(c => c.id)), new Set(Object.keys(all)));
  assert.deepEqual(Object.fromEntries(Object.entries(constants).map(([k, v]) => [k.replace('_', ':'), v])), all);
  for (const id of ['EPSG:28992', 'ESRI:54009']) {
    const path = id.replace(':', '/');
    const module = await import(`crs-defs/${path}`);
    const record = await readJson(`${path}.json`);
    assert.equal(module.definition, all[id]);
    assert.equal(record.definition, all[id]);
    assert.equal(module.id, id);
  }
});
