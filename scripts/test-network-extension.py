#!/usr/bin/env python3
"""Local fixture only: compiles policy tests and refuses DNS at a mock proxy."""
import os, pathlib, socket, subprocess, tempfile, threading
root=pathlib.Path(__file__).resolve().parent.parent
with tempfile.TemporaryDirectory(prefix='cg-network-test-') as tmp:
    exe=pathlib.Path(tmp)/'policy-tests'
    subprocess.run(['xcrun','swiftc','-swift-version','5','-o',str(exe),str(root/'network-extension/Policy.swift'),str(root/'network-extension/DNSOverProxy.swift'),str(root/'tests/NetworkPolicyTests.swift'),'-framework','CFNetwork'],check=True)
    server=socket.socket();server.bind(('127.0.0.1',0));server.listen();server.settimeout(16)
    observed=[]
    def refuse():
        try:
            while True:
                conn,_=server.accept()
                with conn:
                    conn.settimeout(2);data=b''
                    while b'\r\n\r\n' not in data and len(data)<8192:
                        part=conn.recv(1024)
                        if not part:break
                        data+=part
                    observed.append(data.split(b'\r\n')[0])
                    conn.sendall(b'HTTP/1.1 502 Bad Gateway\r\nContent-Length: 0\r\nConnection: close\r\n\r\n')
        except (OSError,TimeoutError):pass
    thread=threading.Thread(target=refuse,daemon=True);thread.start()
    env=dict(os.environ,CG_TEST_PROXY_PORT=str(server.getsockname()[1]))
    subprocess.run([str(exe)],env=env,check=True,timeout=20)
    server.close()
    assert observed and all(line==b'CONNECT 1.1.1.1:443 HTTP/1.1' for line in observed), observed
    print('Confirmed DNS transport connects to the explicit loopback proxy with CONNECT; no hostname bootstrap was required.')
