#!/usr/bin/env python3
"""Compile only, or embed and Developer-ID-sign the experimental network module.
No activation, no DNS/proxy/firewall preference mutation. Profiles never enter source.
"""
import argparse, datetime, json, pathlib, plistlib, shutil, subprocess, tempfile
ROOT = pathlib.Path(__file__).resolve().parent.parent

def run(args):
    subprocess.run([str(a) for a in args], check=True)

def clean_generated_metadata(app):
    # Only generated bundle sidecars, never source files or user profiles.
    for sidecar in app.rglob('._*'):
        try:
            if sidecar.is_file() and sidecar.read_bytes()[:4] == b'\x00\x05\x16\x07':
                sidecar.unlink()
        except FileNotFoundError:
            pass

def compile_native(output):
    source = ROOT / 'network-extension'
    common = ['xcrun','swiftc','-swift-version','5','-target',subprocess.check_output(['uname','-m'],text=True).strip()+'-apple-macos12.0']
    run(common + ['-module-name','ClaudeGuardNetwork','-o',output/'extension',source/'Policy.swift',source/'FilterProvider.swift',source/'DNSOverProxy.swift',source/'DNSProvider.swift',source/'main.swift','-framework','NetworkExtension','-framework','Security','-framework','CFNetwork'])
    run(common + ['-o',output/'controller',source/'Policy.swift',source/'Controller.swift','-framework','NetworkExtension','-framework','SystemExtensions'])

def profile(path, bundle, required):
    data=plistlib.loads(subprocess.check_output(['/usr/bin/security','cms','-D','-i',str(path)],stderr=subprocess.DEVNULL))
    e=data['Entitlements'];team=data['TeamIdentifier'][0]
    if e.get('com.apple.application-identifier') != team+'.'+bundle:
        raise SystemExit('Provisioning profile does not match bundle identifier: '+bundle)
    if data['ExpirationDate'] <= datetime.datetime.now(datetime.timezone.utc).replace(tzinfo=None):
        raise SystemExit('Expired provisioning profile')
    supported=e.get('com.apple.developer.networking.networkextension',[])
    if not set(required).issubset(supported):raise SystemExit('Profile lacks required Network Extension capabilities')
    return e,team

def main():
    p=argparse.ArgumentParser();p.add_argument('--app',type=pathlib.Path);p.add_argument('--identity');p.add_argument('--app-profile',type=pathlib.Path);p.add_argument('--extension-profile',type=pathlib.Path);a=p.parse_args()
    with tempfile.TemporaryDirectory(prefix='cg-network-build-') as tmp:
        tmp=pathlib.Path(tmp);compile_native(tmp)
        if not a.app:
            print('Native controller and both providers compile. No extension installed or activated.');return
        if not all([a.identity,a.app_profile,a.extension_profile]):raise SystemExit('Signing identity and both profiles are required; ad-hoc activation is not supported.')
        app=a.app.resolve();info_path=app/'Contents/Info.plist';info=plistlib.loads(info_path.read_bytes());bundle=info['CFBundleIdentifier'];extension_id=bundle+'.network'
        required=['content-filter-provider-systemextension','dns-proxy-systemextension']
        ae,team=profile(a.app_profile,bundle,required);ee,extension_team=profile(a.extension_profile,extension_id,required)
        if team!=extension_team or not ae.get('com.apple.developer.system-extension.install'):raise SystemExit('Profiles must share a team and host must permit system-extension installation')
        # Use minimal entitlements; keep provisioned IDs, never copy debug/task access.
        def entitlements(e):
            keep=['com.apple.application-identifier','com.apple.developer.team-identifier','com.apple.developer.networking.networkextension','com.apple.developer.system-extension.install']
            return {k:e[k] for k in keep if k in e}
        ae=entitlements(ae);ee=entitlements(ee)
        ae_file=tmp/'app.entitlements';ee_file=tmp/'extension.entitlements';ae_file.write_bytes(plistlib.dumps(ae));ee_file.write_bytes(plistlib.dumps(ee))
        ext=app/'Contents/Library/SystemExtensions'/f'{extension_id}.systemextension';macos=ext/'Contents/MacOS';macos.mkdir(parents=True,exist_ok=True)
        shutil.copy2(tmp/'extension',macos/'claudeguard-network-extension');shutil.copy2(a.extension_profile,ext/'Contents/embedded.provisionprofile')
        ext_info={'CFBundleIdentifier':extension_id,'CFBundleExecutable':'claudeguard-network-extension','CFBundleName':'ClaudeGuard Network','CFBundlePackageType':'SYSX','CFBundleVersion':info['CFBundleVersion'],'CFBundleShortVersionString':info['CFBundleShortVersionString'],'LSMinimumSystemVersion':'12.0','NSSystemExtensionUsageDescription':'ClaudeGuard 需要网络扩展来阻止所选应用绕过本机代理，并保护系统 DNS。','NetworkExtension':{'NEMachServiceName':team+'.'+extension_id,'NEProviderClasses':{'com.apple.networkextension.filter-data':'FilterProvider','com.apple.networkextension.dns-proxy':'DNSProvider'}}}
        (ext/'Contents/Info.plist').write_bytes(plistlib.dumps(ext_info))
        shutil.copy2(tmp/'controller',app/'Contents/MacOS/claudeguard-network');shutil.copy2(a.app_profile,app/'Contents/embedded.provisionprofile')
        info['NSSystemExtensionUsageDescription']='ClaudeGuard 使用网络扩展阻止绕行连接及 DNS 回退直连。';info['CGNetworkProvisioned']=True;info['CGNetworkExtensionIdentifier']=extension_id;info_path.write_bytes(plistlib.dumps(info))
        for binary,ent in [(ext,ee_file),(app/'Contents/MacOS/claudeguard-network',ae_file),(app,ae_file)]:
            clean_generated_metadata(app)
            run(['/usr/bin/codesign','--force','--options','runtime','--timestamp','--sign',a.identity,'--entitlements',ent,binary])
        run(['/usr/bin/codesign','--verify','--deep','--strict',app])
        print('Signed build prepared. Activation and end-to-end validation are still required.')
if __name__=='__main__':main()
