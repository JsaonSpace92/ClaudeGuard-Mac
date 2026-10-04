#!/usr/bin/env python3
"""Rasterize the menu-bar shield with stdlib only; supersampling preserves Retina edges."""
from pathlib import Path
import math, struct, zlib
size, samples = 44, 8
shield = [(22,2.5),(40,9),(38.5,25),(33,34),(22,41.5),(11,34),(5.5,25),(4,9),(22,2.5)]
check = [(13,22),(19,28),(31,15)]
def distance(x,y,a,b):
 dx,dy=b[0]-a[0],b[1]-a[1]
 t=max(0,min(1,((x-a[0])*dx+(y-a[1])*dy)/(dx*dx+dy*dy)))
 return math.hypot(x-a[0]-t*dx,y-a[1]-t*dy)
def covered(x,y):
 return any(distance(x,y,a,b)<=width/2 for points,width in [(shield,3.4),(check,3.8)] for a,b in zip(points,points[1:]))
rows=bytearray()
for y in range(size):
 rows.append(0)
 for x in range(size):
  n=sum(covered(x+(sx+.5)/samples,y+(sy+.5)/samples) for sy in range(samples) for sx in range(samples))
  rows.extend((0,0,0,round(n*255/(samples*samples))))
def chunk(kind,data):
 return struct.pack('>I',len(data))+kind+data+struct.pack('>I',zlib.crc32(kind+data))
png=b'\x89PNG\r\n\x1a\n'+chunk(b'IHDR',struct.pack('>IIBBBBB',size,size,8,6,0,0,0))+chunk(b'IDAT',zlib.compress(rows))+chunk(b'IEND',b'')
(Path(__file__).resolve().parent.parent/'assets/tray-icon.png').write_bytes(png)
