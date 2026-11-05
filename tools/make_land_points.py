#!/usr/bin/env python3
"""Builds assets/land-points.bin from Natural Earth's public-domain 1:110m land.

Usage: make_land_points.py ne_110m_land.geojson assets/land-points.bin

Spreads points evenly over a sphere, keeps those on land, and writes each as a
little-endian pair of int16: latitude and longitude in hundredths of a degree.
"""
import json
import math
import struct
import sys

POINTS = 16000


def rings(path):
    with open(path, encoding="utf-8") as source:
        for feature in json.load(source)["features"]:
            geometry = feature["geometry"]
            polygons = geometry["coordinates"]
            if geometry["type"] == "Polygon":
                polygons = [polygons]
            for polygon in polygons:
                yield polygon[0]  # At this scale, land has no holes that matter.


def inside(lon, lat, ring):
    hit = False
    for (x1, y1), (x2, y2) in zip(ring, ring[1:]):
        if (y1 > lat) != (y2 > lat) and lon < (x2 - x1) * (lat - y1) / (y2 - y1) + x1:
            hit = not hit
    return hit


def main():
    land = []
    for ring in rings(sys.argv[1]):
        xs = [p[0] for p in ring]
        ys = [p[1] for p in ring]
        land.append((min(xs), max(xs), min(ys), max(ys), ring))

    golden = math.pi * (3 - math.sqrt(5))
    kept = []
    for i in range(POINTS):
        lat = math.degrees(math.asin(1 - 2 * (i + 0.5) / POINTS))
        lon = math.degrees((i * golden) % (2 * math.pi)) - 180
        for left, right, bottom, top, ring in land:
            if left <= lon <= right and bottom <= lat <= top and inside(lon, lat, ring):
                kept.append((lat, lon))
                break

    with open(sys.argv[2], "wb") as out:
        for lat, lon in kept:
            out.write(struct.pack("<hh", round(lat * 100), round(lon * 100)))
    print(f"{len(kept)} land points of {POINTS}")


if __name__ == "__main__":
    main()
