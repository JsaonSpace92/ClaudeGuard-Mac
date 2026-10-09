//! Bounded RFC 1928 UDP relay test. Only a saved loopback SOCKS endpoint is
//! contacted. Keep its TCP association alive; never fall back to a remote relay.
use std::io::{Read, Write};
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr, TcpStream, UdpSocket};
use std::time::{Duration, Instant};

fn reply_address(stream: &mut TcpStream) -> Result<SocketAddr, String> {
    let mut header = [0; 4];
    stream
        .read_exact(&mut header)
        .map_err(|_| "代理未返回 UDP 协商结果")?;
    if header[0] != 5 || header[2] != 0 {
        return Err("UDP 协商响应格式无效".into());
    }
    if header[1] != 0 {
        return Err(format!("代理拒绝 UDP 转发，状态码 {}", header[1]));
    }
    let ip = match header[3] {
        1 => {
            let mut b = [0; 4];
            stream
                .read_exact(&mut b)
                .map_err(|_| "UDP 中继地址不完整")?;
            IpAddr::V4(Ipv4Addr::from(b))
        }
        4 => {
            let mut b = [0; 16];
            stream
                .read_exact(&mut b)
                .map_err(|_| "UDP 中继地址不完整")?;
            IpAddr::V6(Ipv6Addr::from(b))
        }
        _ => return Err("代理未提供可验证的本机 UDP 中继 IP".into()),
    };
    let mut port = [0; 2];
    stream
        .read_exact(&mut port)
        .map_err(|_| "UDP 中继端口不完整")?;
    Ok(SocketAddr::new(ip, u16::from_be_bytes(port)))
}

pub fn payload(packet: &[u8]) -> Option<&[u8]> {
    if packet.get(..3)? != [0, 0, 0] {
        return None;
    }
    let offset = match *packet.get(3)? {
        1 => 10,
        4 => 22,
        3 => 7 + usize::from(*packet.get(4)?),
        _ => return None,
    };
    packet.get(offset..)
}

pub fn exchange(proxy: SocketAddr, host: &str, port: u16, query: &[u8]) -> Result<Vec<u8>, String> {
    if !proxy.ip().is_loopback() || proxy.port() == 0 || host.is_empty() || host.len() > 255 {
        return Err("仅允许已配置的本机 SOCKS 代理".into());
    }
    let mut tcp = TcpStream::connect_timeout(&proxy, Duration::from_secs(3))
        .map_err(|_| "无法连接本机 SOCKS 入口")?;
    tcp.set_read_timeout(Some(Duration::from_secs(3)))
        .map_err(|_| "无法设置协商期限")?;
    tcp.set_write_timeout(Some(Duration::from_secs(3)))
        .map_err(|_| "无法设置协商期限")?;
    tcp.write_all(&[5, 1, 0])
        .map_err(|_| "SOCKS 协商发送失败")?;
    let mut greeting = [0; 2];
    tcp.read_exact(&mut greeting)
        .map_err(|_| "本机入口不支持 SOCKS5 协商")?;
    if greeting != [5, 0] {
        return Err("本机 SOCKS 入口需要认证或不支持此协商".into());
    }
    let udp =
        UdpSocket::bind(SocketAddr::new(proxy.ip(), 0)).map_err(|_| "无法创建本机 UDP 探测端口")?;
    let local = udp.local_addr().map_err(|_| "无法读取探测端口")?;
    let mut request = vec![5, 3, 0];
    match local.ip() {
        IpAddr::V4(ip) => {
            request.push(1);
            request.extend(ip.octets());
        }
        IpAddr::V6(ip) => {
            request.push(4);
            request.extend(ip.octets());
        }
    }
    request.extend(local.port().to_be_bytes());
    tcp.write_all(&request).map_err(|_| "无法申请 UDP 转发")?;
    let mut relay = reply_address(&mut tcp)?;
    if relay.ip().is_unspecified() {
        relay.set_ip(proxy.ip());
    }
    if !relay.ip().is_loopback() || relay.port() == 0 {
        return Err("代理返回非本机中继，已拒绝发送以避免绕行".into());
    }
    udp.connect(relay).map_err(|_| "无法连接本机 UDP 中继")?;
    udp.set_read_timeout(Some(Duration::from_millis(650)))
        .map_err(|_| "无法设置 UDP 期限")?;
    udp.set_write_timeout(Some(Duration::from_secs(1)))
        .map_err(|_| "无法设置 UDP 期限")?;
    let mut packet = vec![0, 0, 0, 3, host.len() as u8];
    packet.extend(host.bytes());
    packet.extend(port.to_be_bytes());
    packet.extend(query);
    let deadline = Instant::now() + Duration::from_secs(5);
    let mut received = [0; 2048];
    for _ in 0..3 {
        udp.send(&packet).map_err(|_| "UDP 中继发送失败")?;
        let round = Instant::now() + Duration::from_millis(1300);
        while Instant::now() < round && Instant::now() < deadline {
            if let Ok(size) = udp.recv(&mut received) {
                if let Some(body) = payload(&received[..size]) {
                    if body.len() >= 20 && body[..2] == [1, 1] && body[4..20] == query[4..20] {
                        return Ok(body.to_vec());
                    }
                }
            }
        }
    }
    Err("本机接受 UDP 协商，但三次探测均未收到有效回包；代理链或远端服务仍需核验".into())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::TcpListener;
    #[test]
    fn rejects_remote_relays_fragments_and_truncated_headers() {
        assert!(exchange(
            "203.0.113.1:1080".parse().unwrap(),
            "example.org",
            3478,
            &[0; 20]
        )
        .is_err());
        assert!(payload(&[0, 0, 1, 1, 0, 0, 0, 0, 0, 0]).is_none());
        assert!(payload(&[0, 0, 0, 3, 50, 0]).is_none());
        assert_eq!(
            payload(&[0, 0, 0, 1, 127, 0, 0, 1, 0, 1, 42]),
            Some(&[42][..])
        );
    }
    #[test]
    fn refuses_remote_relay_returned_by_local_proxy() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let proxy = listener.local_addr().unwrap();
        let worker = std::thread::spawn(move || {
            let (mut tcp, _) = listener.accept().unwrap();
            tcp.set_read_timeout(Some(Duration::from_secs(3))).unwrap();
            let mut greeting = [0; 3];
            tcp.read_exact(&mut greeting).unwrap();
            tcp.write_all(&[5, 0]).unwrap();
            let mut request = [0; 10];
            tcp.read_exact(&mut request).unwrap();
            tcp.write_all(&[5, 0, 0, 1, 203, 0, 113, 1, 4, 56]).unwrap();
            let mut byte = [0];
            assert_eq!(tcp.read(&mut byte).unwrap(), 0);
        });
        assert!(exchange(proxy, "stun.example.org", 3478, &[0; 20])
            .unwrap_err()
            .contains("非本机"));
        worker.join().unwrap();
    }
    #[test]
    fn owned_relay_round_trip_keeps_control_connection_and_domain() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let proxy = listener.local_addr().unwrap();
        let worker = std::thread::spawn(move || {
            let (mut tcp, _) = listener.accept().unwrap();
            tcp.set_read_timeout(Some(Duration::from_secs(3))).unwrap();
            let mut greeting = [0; 3];
            tcp.read_exact(&mut greeting).unwrap();
            assert_eq!(greeting, [5, 1, 0]);
            tcp.write_all(&[5, 0]).unwrap();
            let mut request = [0; 10];
            tcp.read_exact(&mut request).unwrap();
            assert_eq!(&request[..4], &[5, 3, 0, 1]);
            let relay = UdpSocket::bind("127.0.0.1:0").unwrap();
            relay
                .set_read_timeout(Some(Duration::from_secs(3)))
                .unwrap();
            let mut reply = vec![5, 0, 0, 1, 127, 0, 0, 1];
            reply.extend(relay.local_addr().unwrap().port().to_be_bytes());
            tcp.write_all(&reply).unwrap();
            let mut buffer = [0; 2048];
            let (size, client) = relay.recv_from(&mut buffer).unwrap();
            assert_eq!(&buffer[5..5 + buffer[4] as usize], b"stun.example.org");
            let mut body = payload(&buffer[..size]).unwrap().to_vec();
            body[0] = 1;
            body[1] = 1;
            let mut response = vec![0, 0, 0, 1, 127, 0, 0, 1, 0, 1];
            response.extend(body);
            relay.send_to(&response, client).unwrap();
            let mut byte = [0];
            assert_eq!(tcp.read(&mut byte).unwrap(), 0);
        });
        let query = [
            0, 1, 0, 0, 0x21, 0x12, 0xa4, 0x42, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12,
        ];
        assert_eq!(
            exchange(proxy, "stun.example.org", 3478, &query).unwrap()[..2],
            [1, 1]
        );
        worker.join().unwrap();
    }
}
