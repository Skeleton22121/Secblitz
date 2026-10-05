"""Render captured Windows ConPTY cells, never invented UI or source fixtures."""
import hashlib
import json
from pathlib import Path
import re
import sys
import unicodedata
from PIL import Image, ImageDraw, ImageFont

folder = Path(sys.argv[1]).resolve()
output = folder / "screenshots"
output.mkdir(exist_ok=True)
palette = ["#0c0c0c", "#c50f1f", "#13a10e", "#c19c00", "#0037da", "#881798", "#3a96dd", "#cccccc", "#767676", "#e74856", "#16c60c", "#f9f1a5", "#3b78ff", "#b4009e", "#61d6d6", "#f2f2f2"]
fg0, bg0 = palette[7], palette[0]
def color(n):
    if n < 16: return palette[n]
    if n >= 232:
        v = 8 + (n - 232) * 10
        return "#%02x%02x%02x" % (v,v,v)
    n -= 16
    vals = [0,95,135,175,215,255]
    return "#%02x%02x%02x" % (vals[n//36], vals[(n//6)%6], vals[n%6])

def screen(text, width, height):
    blank = lambda: [(" ", fg0, bg0, False) for _ in range(width)]
    rows = [blank() for _ in range(height)]
    x = y = 0; fg=fg0; bg=bg0; bold=False; saved=(0,0); i=0
    def erase(row, start, end):
        for c in range(max(0,start),min(width,end)): rows[row][c] = (" ",fg,bg,bold)
    def linefeed():
        nonlocal y
        y += 1
        if y >= height: rows.pop(0); rows.append(blank()); y=height-1
    while i < len(text):
        c=text[i];i+=1
        if c == "\x1b":
            if i>=len(text): break
            c=text[i];i+=1
            if c=="[":
                start=i
                while i<len(text) and not ("@"<=text[i]<="~"): i+=1
                if i>=len(text): break
                params=text[start:i];cmd=text[i];i+=1
                private=params.startswith("?");nums=[int(n) if n.isdigit() else 0 for n in params.lstrip("?").split(";")]
                n=nums[0] or 1
                if cmd in "Hf": y=max(0,min(height-1,n-1));x=max(0,min(width-1,(nums[1] if len(nums)>1 and nums[1] else 1)-1))
                elif cmd=="A": y=max(0,y-n)
                elif cmd=="B": y=min(height-1,y+n)
                elif cmd=="C": x=min(width-1,x+n)
                elif cmd=="D": x=max(0,x-n)
                elif cmd=="G": x=max(0,min(width-1,n-1))
                elif cmd=="d": y=max(0,min(height-1,n-1))
                elif cmd=="J":
                    if nums[0] in (2,3): rows=[blank() for _ in range(height)]
                    elif nums[0]==0:
                        erase(y,x,width)
                        for r in range(y+1,height): erase(r,0,width)
                    elif nums[0]==1:
                        for r in range(y): erase(r,0,width)
                        erase(y,0,x+1)
                elif cmd=="K":
                    if nums[0]==0: erase(y,x,width)
                    elif nums[0]==1: erase(y,0,x+1)
                    elif nums[0]==2: erase(y,0,width)
                elif cmd=="X": erase(y,x,x+n)
                elif cmd=="m":
                    j=0
                    while j<len(nums):
                        v=nums[j];j+=1
                        if v==0: fg,bg,bold=fg0,bg0,False
                        elif v==1: bold=True
                        elif v==22: bold=False
                        elif v==39: fg=fg0
                        elif v==49: bg=bg0
                        elif 30<=v<=37: fg=palette[v-30]
                        elif 90<=v<=97: fg=palette[v-90+8]
                        elif 40<=v<=47: bg=palette[v-40]
                        elif 100<=v<=107: bg=palette[v-100+8]
                        elif v in (38,48) and j+1<len(nums) and nums[j]==5:
                            value=color(nums[j+1]);j+=2
                            if v==38: fg=value
                            else: bg=value
                        elif v in (38,48) and j+3<len(nums) and nums[j]==2:
                            value="#%02x%02x%02x"%tuple(nums[j+1:j+4]);j+=4
                            if v==38: fg=value
                            else: bg=value
                elif cmd=="s": saved=(x,y)
                elif cmd=="u": x,y=saved
                elif cmd=="h" and private and 1049 in nums: rows=[blank() for _ in range(height)];x=y=0
            elif c in "]P_^":
                while i<len(text) and text[i]!="\x07" and text[i:i+2]!="\x1b\\": i+=1
                i += 2 if text[i:i+2]=="\x1b\\" else 1
            elif c in "78":
                if c=="7":saved=(x,y)
                else:x,y=saved
            elif c=="M": y=max(0,y-1)
            elif c in "()": i+=1
            continue
        if c=="\r":x=0;continue
        if c=="\n":linefeed();continue
        if c=="\b":x=max(0,x-1);continue
        if c=="\t":x=min(width-1,(x//8+1)*8);continue
        if ord(c)<32 or ord(c)==127:continue
        if unicodedata.combining(c):
            if x>0:
                old=rows[y][x-1];rows[y][x-1]=(old[0]+c,*old[1:])
            continue
        cells=2 if unicodedata.east_asian_width(c) in "WF" else 1
        if x>=width or x+cells>width:x=0;linefeed()
        rows[y][x]=(c,fg,bg,bold)
        if cells==2:rows[y][x+1]=("",fg,bg,bold)
        x+=cells
    return rows

regular=ImageFont.truetype("/usr/share/fonts/truetype/dejavu/DejaVuSansMono.ttf",20)
heavy=ImageFont.truetype("/usr/share/fonts/truetype/dejavu/DejaVuSansMono-Bold.ttf",20)
manifest=[]
for meta in sorted(folder.glob("*.frame.json")):
    props=json.loads(meta.read_text(encoding="utf-8-sig")); w,h=props["width"],props["height"]
    capture=meta.with_name(meta.name.replace(".frame.json",".vt"))
    data=capture.read_bytes();rows=screen(data.decode("utf-8",errors="replace"),w,h)
    cellw,cellh,pad=13,27,18
    image=Image.new("RGB",(w*cellw+pad*2,h*cellh+pad*2),bg0);draw=ImageDraw.Draw(image)
    for r,row in enumerate(rows):
        for col,(char,fg,bg,bold) in enumerate(row):
            if char=="": continue # Continuation cell of a wide glyph.
            x,y=pad+col*cellw,pad+r*cellh
            draw.rectangle((x,y,x+cellw-1,y+cellh-1),fill=bg)
            if char and char!=" ":draw.text((x,y),char,font=heavy if bold else regular,fill=fg)
    name=meta.name.replace(".frame.json","")
    path=output/(name+".png");image.save(path)
    plain="\n".join("".join(c[0] for c in row).rstrip() for row in rows)
    (output/(name+".txt")).write_text(plain+"\n")
    colors=sorted({c[1] for row in rows for c in row if c[0].strip()})
    record={"png":path.name,"sha256":hashlib.sha256(path.read_bytes()).hexdigest(),"vt_sha256":hashlib.sha256(data).hexdigest(),"width":w,"height":h,"text_colors":colors,"no_color":props["no_color"],"source":"Actual Windows ConPTY output; terminal-cell PNG rendered on host with DejaVu Sans Mono"}
    manifest.append(record)
(output/"manifest.json").write_text(json.dumps(manifest,indent=2)+"\n")
print("Rendered %d Windows ConPTY captures; hashes: %s" % (len(manifest),output/"manifest.json"))
