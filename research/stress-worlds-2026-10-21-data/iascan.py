import json,sys,os,io,zipfile,urllib.request,concurrent.futures as cf
sys.path.insert(0,os.path.dirname(os.path.abspath(__file__)))
from rzip import HF
ids=json.load(open('/tmp/ia_ids.json'))
cand=[(i,t,s) for i,(t,s) in ids.items() if 80e6<s<80e9]
print('candidates',len(cand),flush=True)
def work(c):
    i,t,s=c
    try:
        m=json.load(urllib.request.urlopen('https://archive.org/metadata/'+i,timeout=40))
    except Exception as e: return (i,t,s,'metaerr',[])
    out=[]
    for f in m.get('files',[]):
        n=f['name']; sz=int(f.get('size',0) or 0)
        if n.lower().endswith('.zip') and sz>50e6 and f.get('source')=='original':
            url='https://archive.org/download/%s/%s'%(i,urllib.parse.quote(n))
            try:
                z=zipfile.ZipFile(io.BufferedReader(HF(url),buffer_size=1<<16))
                hits=[(x.file_size,x.filename) for x in z.infolist() if x.filename.endswith(('create_tracks.dat','create_logistics.dat'))]
                out.append((n,sz,len(z.infolist()),hits))
            except Exception as e:
                out.append((n,sz,-1,[('err',str(e)[:60])]))
        elif n.lower().endswith(('.7z','.tar','.tar.gz','.tgz','.tar.zst','.rar')) and sz>50e6:
            out.append((n,sz,-2,[]))
    return (i,t,s,'ok',out)
import urllib.parse
res=[]
with cf.ThreadPoolExecutor(8) as ex:
    for r in ex.map(work,cand):
        res.append(r)
        i,t,s,st,out=r
        for (n,sz,ne,hits) in out:
            if hits and hits[0][0]!='err': print('HIT',i,'|',t,'|',n,round(sz/1e6),'MB',ne,hits,flush=True)
json.dump(res,open('/tmp/ia_scan.json','w'))
print('done',len(res))
