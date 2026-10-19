/** The code with its authority, such as `EPSG:28992`. */
export declare const id: string;

/** The CRS's name in its registry. */
export declare const name: string;

/** Area of use in degrees: west, south, east, north. West is greater than east across the antimeridian. */
export declare const bbox: [west: number, south: number, east: number, north: number];

/** PROJ string for proj4js and PROJ, including the datum shift to WGS 84. */
export declare const definition: string;

/** Largest distance in meters from PROJ's best transformation, over sample points in the area of use. */
export declare const errorM: number;
