# Notices

## EPSG data

The `EPSG/` definitions are derived from the EPSG Geodetic Parameter Dataset, which is owned by the International Association of Oil and Gas Producers (IOGP). They were read from the copy of the dataset in PROJ's database. The version is in `catalog.json`.

A definition marked `"approximate": true` in its record is more than 1 cm from PROJ's best transformation for that CRS. It is not the EPSG Dataset's own definition and must not be attributed to it.

The EPSG Dataset is used under the EPSG Dataset Terms of Use, <https://epsg.org/terms-of-use.html>. Anyone who receives these definitions is bound by those terms. The general terms, revised 8 April 2016, read:

> The EPSG Facilities are published by IOGP at no charge. Distribution for profit is forbidden.
>
> The EPSG Facilities are owned by IOGP. They are compiled by the Geodetic Subcommittee of the IOGP from publicly available and member-supplied information.
>
> The data may be used, copied and distributed subject to the following conditions:
>
> Whilst every effort has been made to ensure the accuracy of the information contained in the EPSG Facilities, neither the IOGP nor any of its members past present or future warrants their accuracy or will, regardless of its or their negligence, assume liability for any foreseeable or unforeseeable use made thereof, which liability is hereby excluded. Consequently, such use is at your own risk. You are obliged to inform anyone to whom you provide the EPSG Facilities of these Terms of Use.
>
> DATA AND INFORMATION PROVIDED IN THE EPSG FACILITIES ARE PROVIDED "AS IS" WITHOUT WARRANTY OF ANY KIND, EITHER EXPRESSED OR IMPLIED, INCLUDING BUT NOT LIMITED TO THE IMPLIED WARRANTIES OF MERCHANTABILITY AND/OR FITNESS FOR A PARTICULAR PURPOSE.
>
> The data may be included in any commercial package provided that any commerciality is based on value added by the provider and not on a value ascribed to the EPSG Dataset which is made available at no charge.
>
> Ownership of the EPSG Dataset by IOGP must be acknowledged in any publication or transmission (by whatever means) thereof (including permitted modifications).
>
> Subsets of information may be extracted from the dataset. Users are advised that coordinate reference system and coordinate transformation descriptions are incomplete unless all elements detailed as essential in IOGP Surveying and Positioning Guidance Note 7-1 Annex A are included.
>
> Essential elements should preferably be reproduced as described in the dataset. Modification of parameter values is permitted as described in the table below to allow change to the content of the information provided that numeric equivalence is achieved. Numeric equivalence refers to the results of geodetic calculations in which the parameters are used, for example (i) conversion of ellipsoid defining parameters, or (ii) conversion of parameters between one and two standard parallel projection methods, or (iii) conversion of parameters between 7-parameter geocentric transformation methods.
>
> No data that has been modified other than as permitted in these Terms of Use shall be attributed to the EPSG Dataset.

The table of permitted modifications is on the page linked above.

## ESRI data

The `ESRI/` definitions are derived from Esri's projection engine database, read from the copy in PROJ's database.

Copyright © 2010-2026 Esri. Licensed under the Apache License, Version 2.0. A copy of the license is in `licenses/esri-apache-2.0.txt` and at <https://www.apache.org/licenses/LICENSE-2.0>. The source is <https://github.com/Esri/projection-engine-db-doc>.
