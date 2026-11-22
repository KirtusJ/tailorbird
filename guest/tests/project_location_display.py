"""Check every destination glyph against the pinned font, not another rendering.

The real Rust example draws a 120-byte accepted path. This independent oracle
also removes row two and changes its last character to prove both fail.
"""
import argparse
import json
from pathlib import Path
from demo_disclosure import expected, label_rgb, replace_region, TEXT, PANEL, SOFT
from qemu_smoke import crop
from live_work_native import ppm

if not __debug__:
    raise SystemExit('Refuse optimized Python for exact destination display checks')

TARGET = 'a' * 116 + '/p/z'
SIZES = ((640,480),(800,600),(1024,768),(1920,1080))


def require(size, pixels):
    bounds, wanted = expected(size)
    assert crop(pixels,size[0],bounds) == wanted, 'missing sample disclosure/context'
    for row, line in enumerate((TARGET[:76], TARGET[76:])):
        bounds, wanted = label_rgb(size,line,308,433+row*16,TEXT,PANEL)
        assert crop(pixels,size[0],bounds) == wanted, f'destination row {row+1} differs'
    for label, x, ink in (('Revision',684,SOFT),('18446744073709551615',756,TEXT)):
        bounds, wanted = label_rgb(size,label,x,150,ink,PANEL)
        assert crop(pixels,size[0],bounds) == wanted, 'revision clipped or overlaps destination'


def check(folder):
    adverse = 0
    for size in SIZES:
        actual, pixels = ppm(folder/f'{size[0]}x{size[1]}.ppm')
        assert actual == size
        require(size,pixels)
        line = TARGET[76:]
        bounds, wanted = label_rgb(size,line,308,449,TEXT,PANEL)
        for replacement in (PANEL*(len(wanted)//3), label_rgb(size,line[:-1]+'x',308,449,TEXT,PANEL)[1]):
            changed = replace_region(pixels,size[0],bounds,replacement)
            try:
                require(size,changed)
            except AssertionError:
                adverse += 1
            else:
                raise AssertionError('altered/missing final destination row passed')
    return {'sizes_checked':len(SIZES),'exact_target_bytes':len(TARGET),'adverse_controls':adverse}


if __name__ == '__main__':
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('folder',type=Path)
    args=parser.parse_args()
    result=check(args.folder)
    (args.folder/'display-result.json').write_text(json.dumps(result,indent=2)+'\n',encoding='utf-8')
    print(json.dumps(result,indent=2))
