import json,urllib.request,urllib.parse,sys
qs=[
 'minecraft AND (title:world OR title:smp OR title:server OR title:season OR title:backup OR title:save OR title:map) AND mediatype:(data OR software)',
 '(minecraft OR "mine craft") AND (world OR smp OR server) AND mediatype:data',
 'title:(smp) AND (minecraft OR mc) AND mediatype:(data OR software)',
 'title:("world download") AND (minecraft OR mc)',
 'title:(world) AND description:(minecraft) AND mediatype:(data OR software)',
 'description:(create) AND description:(minecraft) AND (world OR smp) AND mediatype:(data OR software)',
 'description:(modded) AND description:(minecraft) AND (world OR smp OR server) AND mediatype:(data OR software)',
 'description:(forge OR neoforge OR fabric) AND description:(minecraft) AND mediatype:(data OR software)',
 'title:(modpack) AND minecraft AND (world OR server) AND mediatype:(data OR software)',
 'subject:(minecraft) AND subject:(smp)', 'subject:(minecraft) AND subject:(world)','subject:(minecraft) AND subject:(create)',
 'subject:(minecraft) AND subject:(modded)', 'subject:(minecraft) AND subject:(server)',
]
ids={}
for q in qs:
    for page in (1,2,3):
        u='https://archive.org/advancedsearch.php?'+urllib.parse.urlencode({'q':q,'fl[]':['identifier','title','item_size'],'rows':200,'page':page,'output':'json'},doseq=True)
        try:d=json.load(urllib.request.urlopen(u,timeout=60))
        except Exception as e: print('err',e); break
        docs=d['response']['docs']
        for x in docs: ids[x['identifier']]=(x.get('title'),int(x.get('item_size',0)))
        if len(docs)<200: break
json.dump(ids,open('/tmp/ia_ids.json','w'))
print(len(ids))
