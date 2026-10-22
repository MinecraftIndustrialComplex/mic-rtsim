"""Remote zip peek. usage: rzip.py <url> ls | cat <member> | summary"""
import sys,io,zipfile,urllib.request,collections
class HF(io.RawIOBase):
    def __init__(s,url):
        s.url=url; r=urllib.request.urlopen(urllib.request.Request(url,method='HEAD'),timeout=60); s.size=int(r.headers['Content-Length']); s.pos=0; s.final=r.geturl()
    def seekable(s): return True
    def readable(s): return True
    def tell(s): return s.pos
    def seek(s,o,w=0):
        s.pos={0:o,1:s.pos+o,2:s.size+o}[w]; return s.pos
    def read(s,n=-1):
        if n<0 or s.pos+n>s.size: n=s.size-s.pos
        if n<=0: return b''
        req=urllib.request.Request(s.final,headers={'Range':'bytes=%d-%d'%(s.pos,s.pos+n-1)})
        b=urllib.request.urlopen(req,timeout=120).read(); s.pos+=len(b); return b
    def readinto(s,b):
        d=s.read(len(b)); b[:len(d)]=d; return len(d)
if __name__=='__main__':
    z=zipfile.ZipFile(io.BufferedReader(HF(sys.argv[1]),buffer_size=1<<16))
    cmd=sys.argv[2]
    if cmd=='ls':
        for i in z.infolist(): print(i.file_size,i.compress_size,i.filename)
    elif cmd=='cat':
        sys.stdout.buffer.write(z.read(sys.argv[3]))
    elif cmd=='summary':
        top=collections.Counter(); sz=collections.Counter(); n=0
        for i in z.infolist():
            p=i.filename.split('/'); k='/'.join(p[:3]) if len(p)>2 else '/'.join(p[:-1])
            top[k]+=1; sz[k]+=i.compress_size
        for k,v in sorted(sz.items(),key=lambda x:-x[1])[:60]: print(round(v/1e6),'MB',top[k],'files',k)
