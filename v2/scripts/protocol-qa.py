#!/usr/bin/env python3
"""Actual OS-pipe SDK + real loopback HTTP/TLS. No OS services or product traffic.
Uses only the explicitly feature-gated bitly-qa-fixture. For packaged production
binary use --production; those cases stop at local validation, without HTTP.
"""
import hashlib, argparse, http.server, json, os, pathlib, queue, ssl, subprocess, threading, time, uuid
ROOT=pathlib.Path(__file__).resolve().parents[1]
QA=ROOT/'qa'; QA.mkdir(exist_ok=True)
TOKEN='synthetic-protocol-credential'
ROW={'id':'bit.ly/test','link':'https://bit.ly/test','long_url':'https://example.test/path','title':'<img src=x> ![remote](https://evil.test/x)','created_at':'2026-01-02T03:04:05Z'}
results=[]
class Server:
    def __init__(self,tls=False):
        self.requests=[]; self.lock=threading.Lock(); self.fail=True
        outer=self
        class Handler(http.server.BaseHTTPRequestHandler):
            protocol_version='HTTP/1.1'
            def log_message(self,*a): pass
            def do_POST(self): self.answer()
            def do_GET(self): self.answer()
            def answer(self):
                raw=self.rfile.read(int(self.headers.get('content-length','0')))
                with outer.lock: outer.requests.append((self.command,self.path,self.headers.get('Authorization'),raw))
                if 'slow' in self.path or self.command=='POST' and b'cancel' in raw: time.sleep(.2)
                if self.command=='POST' and b'unknown-timeout' in raw: time.sleep(16)
                status=200; response={'link':'https://bit.ly/prepared'} if self.command=='POST' else {'links':[ROW], 'pagination':{'next':'https://evil.test/never-follow'}}
                if 'empty' in self.path: response={'links':[]}
                if 'fail' in self.path and outer.fail: status=503;response={'message':'untrusted remote detail'};outer.fail=False
                if 'redirect' in self.path: status=302
                body=json.dumps(response).encode(); self.send_response(status)
                if status==302:self.send_header('Location','https://evil.test/never-follow')
                if 'oversize' in self.path:self.send_header('Content-Length',str(3*1024*1024))
                else:self.send_header('Content-Length',str(len(body)))
                self.send_header('Content-Type','application/json');self.send_header('Connection','close');self.end_headers()
                try:self.wfile.write(body[:4] if 'partial' in self.path else body)
                except (BrokenPipeError,ConnectionResetError):pass
        self.http=http.server.ThreadingHTTPServer(('127.0.0.1',0),Handler)
        self.http.daemon_threads=False
        if tls:
            context=ssl.SSLContext(ssl.PROTOCOL_TLS_SERVER);context.load_cert_chain(ROOT/'tests/tls/server.pem',ROOT/'tests/tls/server-key.pem');self.http.socket=context.wrap_socket(self.http.socket,server_side=True)
        self.origin=f'{"https" if tls else "http"}://127.0.0.1:{self.http.server_port}/v4/'
        self.thread=threading.Thread(target=self.http.serve_forever);self.thread.start()
    def close(self): self.http.shutdown();self.http.server_close();self.thread.join(3);assert not self.thread.is_alive()
class Peer:
    def __init__(self,binary,origin=None,cert=False,label='peer'):
        self.path=QA/(label+'-'+uuid.uuid4().hex[:8]);self.path.mkdir(exist_ok=True)
        for p in ['home','tmp','data']: (self.path/p).mkdir(exist_ok=True)
        env={k:v for k,v in os.environ.items() if k in ['PATH','SYSTEMROOT','WINDIR','LANG']};env.update(HOME=str(self.path/'home'),TMPDIR=str(self.path/'tmp'))
        if origin:env['BITLY_QA_ORIGIN']=origin
        if cert:env['BITLY_QA_CERT']=str(ROOT/'tests/tls/server.pem')
        self.p=subprocess.Popen([str(binary)],stdin=subprocess.PIPE,stdout=subprocess.PIPE,stderr=subprocess.PIPE,env=env)
        self.frames=queue.Queue();self.saved=[];self.host=[];self.n=0
        def reader():
            for l in self.p.stdout:self.frames.put(json.loads(l))
        self.reader=threading.Thread(target=reader);self.reader.start();hello=self.take();assert hello['type']=='hello' and hello['id']=='com.tezbar.native.bitly-url-shortener'
        assert self.ask('initialize',{'protocol':1,'extensionId':hello['id'],'platform':'macOS','packageDir':str(ROOT),'dataDir':str((self.path/'data').resolve()),'tempDir':str((self.path/'tmp').resolve()),'grantedCapabilities':['selection.read','clipboard.read','clipboard.write','clipboard.pasteText','shell.open','settings.open'],'migrationPaths':{}})['result']['ready']
    def take(self,timeout=3):return self.frames.get(timeout=timeout)
    def send(self,v):self.p.stdin.write((json.dumps(v)+'\n').encode());self.p.stdin.flush()
    def request(self,method,params):
        self.n+=1;i='h:'+str(uuid.uuid4());self.send({'type':'request','id':i,'method':method,'params':params});return i
    def reply(self,frame,value):self.send({'type':'response','id':frame['id'],'result':value})
    def wait(self,i,selection='https://example.test/input',ack=None,clipboard='',timeout=3):
        while True:
            for n,v in enumerate(self.saved):
                if v.get('type')=='response' and v.get('id')==i:return self.saved.pop(n)
            v=self.take(timeout)
            if v['type']=='request':
                self.host.append(v)
                value=selection if v['method']=='host.selection.read' else clipboard if v['method']=='host.clipboard.read' else ({'ok':True} if ack is None else ack)
                self.reply(v,value)
            elif v.get('id')==i and v['type']=='response':return v
            else:self.saved.append(v)
    def ask(self,m,p,**kw):return self.wait(self.request(m,p),**kw)
    def open(self,s='s',command='list-links',token=TOKEN,group='group_test',**kw):return self.ask('session.open',{'sessionId':s,'commandName':command,'preferences':{'accessToken':token,'group_guid':group},'argumentValues':{}},**kw)
    def call(self,s,op,**payload):return self.ask('session.call',{'sessionId':s,'operation':'extension:'+op,'payload':payload})
    def close(self,eof=False):
        if self.p.poll() is None:
            if not eof:self.ask('shutdown',{})
            self.p.stdin.close();self.p.wait(timeout=4)
        self.reader.join(3);assert not self.reader.is_alive();self.p.stderr.read();self.p.stdout.close();self.p.stderr.close()
def view(v): assert 'error' not in v,v;assert v['result']['mode']=='view';return v['result']
def wait_requests(server,n):
    end=time.monotonic()+2
    while len(server.requests)<n and time.monotonic()<end:time.sleep(.005)
    assert len(server.requests)>=n

def run(binary):
    server=Server();p=Peer(binary,server.origin,label='protocol-main')
    try:
        r=view(p.open('list')); assert r['refreshMs'] is None
        assert r['root']['children'][0]['props']['title']==ROW['title']
        original=r['actions'][0]['id']; result=p.call('list','invoke-action',actionId=original); assert result['result']['message']=='Done'
        assert p.host[-1]['params']=={'text':ROW['link']};assert 'html' not in p.host[-1]['params']
        assert 'error' in p.call('list','invoke-action',actionId=original)
        # Reader-order latest search wins, including A/B/A with old actions rejected.
        a=p.request('session.call',{'sessionId':'list','operation':'extension:search-text-changed','payload':{'searchText':'slow-A'}});wait_requests(server,2)
        b=p.request('session.call',{'sessionId':'list','operation':'extension:search-text-changed','payload':{'searchText':'B'}})
        c=p.request('session.call',{'sessionId':'list','operation':'extension:search-text-changed','payload':{'searchText':'A'}})
        latest=view(p.wait(c));assert latest['root']['props']['searchText']=='A';assert 'error' in p.wait(a);assert 'error' in p.wait(b)
        assert 'error' in p.call('list','invoke-action',actionId=r['actions'][1]['id'])
        results.append('OS-pipe list, copy literal, one-use IDs, A/B/A latest, stale actions')
        r=view(p.call('list','search-text-changed',searchText='fail'));assert r['actions'][0]['title']=='Retry'
        r=view(p.call('list','invoke-action',actionId=r['actions'][0]['id']));assert r['root']['children'][0]['type']=='List.Item'
        results.append('ordinary completed fetch failure has deliberate working Retry')
        before=len(server.requests);view(p.open('same'));view(p.open('group',group='other_group'));view(p.open('credential',token='different-synthetic-credential'));assert len(server.requests)==before+2
        assert all(x[2] in ['Bearer '+TOKEN,'Bearer different-synthetic-credential'] for x in server.requests)
        assert not any('evil.test' in x[1] for x in server.requests)
        results.append('credential/group cache isolation; no API pagination forwarding')
        # Copy failure saves exact prepared link. Strict ACK rejects bool; retry never POSTs.
        r=view(p.open('short','index',ack=True));before=len(server.requests)
        assert p.call('short','invoke-action',actionId=r['actions'][0]['id'])['result']['mode']=='no-view';assert len(server.requests)==before
        results.append('remote shorten + invalid ACK + exact prepared clipboard retry without POST')
        # Cancel a POST after the server receives it, reopen and refresh without resending.
        i=p.request('session.open',{'sessionId':'cancel','commandName':'index','preferences':{'accessToken':TOKEN},'argumentValues':{}})
        f=p.take();assert f['method']=='host.selection.read';p.reply(f,'https://example.test/cancel');wait_requests(server,before+1)
        p.send({'type':'cancel','requestId':i,'reason':'QA cancellation'});assert 'error' in p.wait(i)
        time.sleep(.25);before=len(server.requests)
        r=view(p.open('recover','index',selection='https://example.test/cancel'));assert r['actions'][0]['title']=='Retry Shortening (may create duplicate)';view(p.call('recover','refresh-session'));assert len(server.requests)==before
        results.append('canceled POST persists uncertainty; reopen/refresh never automatically resends')
        # Cancel an opening reverse copy. Late ACK cannot authorize/commit old view.
        i=p.request('session.open',{'sessionId':'late','commandName':'index','preferences':{'accessToken':TOKEN},'argumentValues':{}})
        f=p.take();assert f['method']=='host.selection.read';p.reply(f,'https://example.test/late')
        f=p.take();assert f['method']=='host.clipboard.write';p.send({'type':'cancel','requestId':i,'reason':'QA copy cancel'});assert 'error' in p.wait(i);p.reply(f,{'ok':True})
        before=len(server.requests);assert p.open('late-recover','index',selection='https://example.test/late')['result']['mode']=='no-view';assert len(server.requests)==before
        results.append('cancel-open/drop cleanup, late ACK ignored, prepared link survives reopen')
        # Dispose while awaiting clipboard; no detached handler holds process shutdown.
        i=p.request('session.open',{'sessionId':'dispose','commandName':'index','preferences':{'accessToken':TOKEN},'argumentValues':{}})
        while True:
            f=p.take()
            if f['type']=='request':break
        p.reply(f,'https://example.test/dispose');f=p.take();assert f['method']=='host.clipboard.write'
        d=p.request('session.dispose',{'sessionId':'dispose'});assert 'error' in p.wait(i);assert 'error' not in p.wait(d);p.reply(f,{'ok':True})
        results.append('dispose during reverse RPC, late reply and shutdown')
        # A completed HTTP timeout is uncertain creation, not permission for an automatic POST retry.
        before=len(server.requests)
        timed=view(p.open('timed','index',selection='https://example.test/unknown-timeout',timeout=20))
        assert len(server.requests)==before+1;assert timed['actions'][0]['title']=='Retry Shortening (may create duplicate)'
        before=len(server.requests);view(p.call('timed','refresh-session'))
        view(p.open('timed-reopen','index',selection='https://example.test/unknown-timeout'));assert len(server.requests)==before
        results.append('actual fifteen-second HTTP timeout preserves uncertainty; refresh/reopen do not resend')
        # More cancellations than the session capacity prove canceled opens release SDK and adapter registrations.
        for n in range(132):
            i=p.request('session.open',{'sessionId':'aborted-'+str(n),'commandName':'index','preferences':{'accessToken':TOKEN},'argumentValues':{}})
            while True:
                f=p.take()
                if f['type']=='request':break
            assert f['method']=='host.selection.read'
            p.send({'type':'cancel','requestId':i,'reason':'QA canceled before input'});assert 'error' in p.wait(i);p.reply(f,'https://example.test/unused')
        results.append('132 canceled opens exceed capacity without leaking SDK/adapter session registrations')
        other=Peer(binary,server.origin,label='isolated-other-profile')
        try:
            before=len(server.requests);view(other.open('profile-list'));assert len(server.requests)==before+1
            before=len(server.requests);assert other.open('profile-short','index')['result']['mode']=='no-view';assert len(server.requests)==before+1
        finally:other.close()
        results.append('simultaneous independent profile processes do not reuse list caches or recovery records')
        # Synthetic body/deadline/redirect failures are deliberate retriable views.
        for text in ['redirect','partial','oversize']:
            r=view(p.call('list','search-text-changed',searchText=text));assert r['actions'][0]['title']=='Retry'
        results.append('redirect, partial body, size overflow rejected over real HTTP')
        # Save actual emitted view for the actual renderer fixture.
        (ROOT/'evidence/protocol-view.json').write_text(json.dumps(latest,indent=2)+'\n')
    finally:p.close();server.close()
    # Actual EOF while awaiting host reverse request.
    server=Server();p=Peer(binary,server.origin,label='protocol-eof')
    try:
        i=p.request('session.open',{'sessionId':'eof','commandName':'index','preferences':{'accessToken':TOKEN},'argumentValues':{}});f=p.take();p.reply(f,'https://example.test/eof');f=p.take();assert f['method']=='host.clipboard.write';p.close(eof=True);results.append('EOF drains pending reverse RPC and exits within four seconds')
    finally:
        if p.p.poll() is None:p.close(eof=True)
        server.close()
    for trusted in [False,True]:
        server=Server(tls=True);p=Peer(binary,server.origin,cert=trusted,label='tls-'+str(trusted))
        try:
            r=view(p.open());assert bool(server.requests)==trusted
            if not trusted:assert r['actions'][0]['title']=='Retry'
        finally:p.close();server.close()
    results.append('TLS certificate verification: untrusted certificate rejected before auth; explicit synthetic CA succeeds')

def production(binary):
    p=Peer(binary,label='production-'+binary.parent.name)
    try:
        r=view(p.open('invalid','index',token=''));assert r['root']['children'][0]['props']['title']=='Invalid Access Token';assert not p.host
        r=view(p.open('input','index',selection='invalid URL'));assert r['root']['children'][0]['props']['description']=='Invalid URL'
        assert [v['method'] for v in p.host]==['host.selection.read']
        # Exercise the real production recovery/copy success path using only synthetic private data.
        literal='https://example.test/production-recovery';parts=[str((p.path/'data').resolve()),TOKEN,literal]
        digest=hashlib.sha256()
        for part in parts:
            b=part.encode();digest.update(len(b).to_bytes(8,'big'));digest.update(b)
        store=p.path/'data/bitly-v2';store.mkdir(exist_ok=True)
        (store/'recovery.json').write_text(json.dumps({digest.hexdigest():{'status':'prepared','link':'https://bit.ly/synthetic-recovery','copied':False}}))
        assert p.open('recovery-success','index',selection=literal)['result']['mode']=='no-view'
        assert p.host[-1]['params']=={'text':'https://bit.ly/synthetic-recovery'}
        assert 'error' in p.ask('session.open',{'sessionId':'unsupported','commandName':'delete','preferences':{},'argumentValues':{}})
        results.append('extracted/production executable: hello/initialize, no-network invalid-token/URL and successful prepared-link copy domain flows, unsupported command')
    finally:p.close()
if __name__=='__main__':
    parser=argparse.ArgumentParser();parser.add_argument('--binary',type=pathlib.Path,default=ROOT/'target/debug/bitly-qa-fixture');parser.add_argument('--production',action='store_true');args=parser.parse_args()
    if args.production:production(args.binary.resolve())
    else:run(args.binary.resolve())
    print(json.dumps({'passed':results},indent=2))
