use std::borrow::Cow;
use std::collections::{HashMap, HashSet};
use std::net::{Ipv4Addr, SocketAddr, SocketAddrV4, UdpSocket};
use std::time::{Duration, Instant};

use crate::light::Light;

pub mod light;
pub mod fields;
pub mod err;

pub const MULTICAST_ADDR: Ipv4Addr = Ipv4Addr::new(239, 255, 255, 250);
pub const MULTICAST_PORT: u16 = 1982;
pub const ALL_LOCAL: Ipv4Addr = Ipv4Addr::new(0, 0, 0, 0);
pub const DEFAULT_LOCAL_PORT: u16 = 7821;

pub const SEARCH_MSG: &'static str = "\
    M-SEARCH * HTTP/1.1\r\n\
    HOST: 239.255.255.250:1982\r\n\
    MAN: \"ssdp:discover\"\r\n\
    ST: wifi_bulb";

#[derive(Debug)]
pub struct YeeClient {
    seeker: UdpSocket,
    multicast_addr: SocketAddrV4,
}

impl YeeClient {
    pub fn new() -> anyhow::Result<YeeClient> {
        let addr = SocketAddrV4::new(MULTICAST_ADDR, MULTICAST_PORT);
        Self::with_addr(addr, DEFAULT_LOCAL_PORT)
    }

    pub fn with_addr(multicast_addr: SocketAddrV4, local_port: u16) -> anyhow::Result<YeeClient> {
        // we don't know the IPs of the lights, so listen to all traffic
        let socket = UdpSocket::bind(SocketAddrV4::new(Ipv4Addr::UNSPECIFIED, local_port))?;
        socket.join_multicast_v4(multicast_addr.ip(), &Ipv4Addr::UNSPECIFIED)?;
        socket.set_nonblocking(true)?;

        Ok(YeeClient { seeker: socket, multicast_addr })
    }

    pub fn get_response(&self, timeout: Duration) -> Vec<Light> {
        // TODO: handle send multicast fail
        self.seeker.send_to(SEARCH_MSG.as_bytes(), &self.multicast_addr).unwrap();

        let mut lights: HashSet<Light> = HashSet::new();
        let now = Instant::now();
        while now.elapsed() < timeout {
            // all lifetimes depend on this buf
            let mut buf = [0u8; 1024];
            let mut headers = [httparse::EMPTY_HEADER; 17];
            let mut res = httparse::Response::new(&mut headers);

            // TODO: use _size to give a slice of buf, thus avoiding cleaning steps
            if let Ok((_size, origin)) = self.seeker.recv_from(&mut buf) {
                let parsed_str = String::from_utf8_lossy(&buf);
                // remove trailing null chars from the array
                let cleaned = parsed_str.trim_matches(char::from(0));

                // TODO: handle if failed to parse response
                res.parse(cleaned.trim().as_bytes()).unwrap();
                let headers: HashMap<&str, Cow<'_, str>> = res.headers.iter()
                    .map(|h| {
                        let name = h.name;
                        let value = String::from_utf8_lossy(h.value);
                        (name, value)
                    }).collect();
                let origin_addr = match origin {
                    SocketAddr::V4(v4) => { v4 }
                    SocketAddr::V6(v6) => panic!("Address of light should not be IPv6: {}", v6)
                };

                match Light::from_fields(&headers, origin_addr) {
                    Ok(new_light) => {

                        // for deduping responses
                        if !lights.contains(&new_light) {
                            lights.insert(new_light);
                        }
                    }
                    Err(_) => {
                        // TODO: handle error
                    }
                }
            }
        }
        let lights: Vec<Light> = lights.into_iter().collect();
        lights
    }
}

#[cfg(test)]
mod tests {
    use std::net::IpAddr;

    use super::*;

    #[test]
    fn is_multicast() {
        assert!(MULTICAST_ADDR.is_multicast());
    }

    #[test]
    fn create_valid_client() {
        // given
        let other_multicast_addr = Ipv4Addr::new(237, 220, 1, 32);
        let other_multicast_port = 1235;
        let sock_addr = SocketAddrV4::new(other_multicast_addr, other_multicast_port);
        let local_port = 5435;

        // when
        let client = YeeClient::with_addr(sock_addr, local_port);

        // then
        assert!(client.is_ok());
        let client = client.unwrap();
        assert_eq!(client.multicast_addr, sock_addr);

        let local_addr = client.seeker.local_addr();
        assert!(local_addr.is_ok());
        let local_addr = local_addr.unwrap();
        assert_eq!(local_addr.ip(), IpAddr::V4(Ipv4Addr::UNSPECIFIED));
        assert_eq!(local_addr.port(), local_port);
    }

    #[test]
    fn create_default_client() {
        // when
        let client = YeeClient::new();

        // then
        assert!(client.is_ok());
        let client = client.unwrap();

        assert_eq!(client.multicast_addr.ip(), &MULTICAST_ADDR);
        assert_eq!(client.multicast_addr.port(), MULTICAST_PORT);

        let local_addr = client.seeker.local_addr();
        assert!(local_addr.is_ok());
        let local_addr = local_addr.unwrap();
        assert_eq!(local_addr.ip(), IpAddr::V4(Ipv4Addr::UNSPECIFIED));
        assert_eq!(local_addr.port(), DEFAULT_LOCAL_PORT);
    }

    #[test]
    fn create_with_invalid_multicast() {
        // given
        let invalid_multicast = SocketAddrV4::new(
            Ipv4Addr::new(223, 0, 0, 255), 80);

        // when
        let client = YeeClient::with_addr(invalid_multicast, 1234);

        // then
        assert!(client.is_err());
    }

    #[test]
    fn send_correct_message() -> anyhow::Result<()> {
        // given
        let client_port = 32742;
        let multicast_port = 8774;
        let fake_multicast_addr = SocketAddrV4::new(Ipv4Addr::LOCALHOST, multicast_port);

        let multicast_listener = UdpSocket::bind(fake_multicast_addr)?;
        let fake_sender = UdpSocket::bind(SocketAddrV4::new(Ipv4Addr::LOCALHOST, client_port))?;
        fake_sender.set_nonblocking(true)?;
        let client = YeeClient { seeker: fake_sender, multicast_addr: fake_multicast_addr };

        // when
        client.get_response(Duration::from_millis(500));

        // then
        let mut recv_buffer = [0; 512];
        multicast_listener.recv_from(&mut recv_buffer)?;
        let mut response = String::from_utf8(recv_buffer.to_vec())?;
        response.retain(|c| c.ne(&char::from(0)));
        assert_eq!(SEARCH_MSG, response.trim());

        Ok(())
    }

    #[test]
    fn find_correct_lights() -> anyhow::Result<()> {
        // GIVEN
        let client_port = 34434;
        let multicast_port = 50945;
        let fake_multicast_addr = SocketAddrV4::new(Ipv4Addr::LOCALHOST, multicast_port);

        // listener just needs to exist, don't need to use
        let _multicast_listener = UdpSocket::bind(fake_multicast_addr)?;
        let client_addr = SocketAddrV4::new(Ipv4Addr::LOCALHOST, client_port);
        let fake_sender = UdpSocket::bind(client_addr)?;

        fake_sender.set_nonblocking(true)?;
        let client = YeeClient { seeker: fake_sender, multicast_addr: fake_multicast_addr };

        // send mock messages
        let fake_port_1 = 63112;
        let fake_light_1 = UdpSocket::bind(SocketAddrV4::new(Ipv4Addr::LOCALHOST, fake_port_1))?;
        // there are already newlines in the string, so need to add \n
        let fake_msg_1 = "HTTP/1.1 200 OK\r
Cache-Control: max-age=3600\r
Date: \r
Ext: \r
Location: yeelight://192.168.1.123:12345\r
Server: POSIX UPnP/1.0 YGLC/1\r
id: 0x12345abcde\r
model: ceiling3\r
fw_ver: 20\r
support: get_prop set_default set_power toggle set_bright set_scene cron_add cron_get cron_del start_cf stop_cf set_ct_abx set_name set_adjust adjust_bright adjust_ct\r
power: on\r
bright: 40\r
color_mode: 2\r
ct: 3300\r
rgb: 2\r
hue: 4\r
sat: 100\r
name: light_one\r\n";
        fake_light_1.send_to(fake_msg_1.as_bytes(), client_addr)?;
        drop(fake_light_1);

        let fake_port_2 = 23449;
        let fake_light_2 = UdpSocket::bind(SocketAddrV4::new(Ipv4Addr::LOCALHOST, fake_port_2))?;
        // there are already newlines in the string, so need to add \n
        let fake_msg_2 = "HTTP/1.1 200 OK\r
Cache-Control: max-age=3600\r
Date: \r
Ext: \r
Location: yeelight://192.168.1.234:54321\r
Server: POSIX UPnP/1.0 YGLC/1\r
id: 0xabcde12345\r
model: lamp\r
fw_ver: 20\r
support: get_prop cron_get cron_del adjust_bright adjust_ct\r
power: off\r
bright: 0\r
color_mode: 1\r
ct: 1000\r
rgb: 125\r
hue: 245\r
sat: 98\r
name: light_one\r\n";
        fake_light_2.send_to(fake_msg_2.as_bytes(), client_addr)?;
        drop(fake_light_2);

        let fake_port_3 = 23449;
        let fake_light_3 = UdpSocket::bind(SocketAddrV4::new(Ipv4Addr::LOCALHOST, fake_port_3))?;
        // there are already newlines in the string, so need to add \n
        let fake_msg_3 = "HTTP/1.1 200 OK\r
Cache-Control: max-age=3600\r
Date: \r
Ext: \r
Location: yeelight://192.168.1.231:12332\r
Server: POSIX UPnP/1.0 YGLC/1\r
id: 0x23498dhf94398h\r
model: mono\r
fw_ver: 20\r
support: \r
power: on\r
bright: 100\r
color_mode: 2\r
ct: 0\r
rgb: 23\r
hue: 34\r
sat: 45\r
name: light_one\r\n";
        fake_light_3.send_to(fake_msg_3.as_bytes(), client_addr)?;
        drop(fake_light_3);

        // WHEN
        let result = client.get_response(Duration::from_millis(500));

        // THEN
        assert_eq!(result.len(), 3);

        Ok(())
    }

    #[test]
    fn return_no_duplicate_lights() -> anyhow::Result<()> {
        // GIVEN
        let client_port = 55461;
        let multicast_port = 9535;
        let fake_multicast_addr = SocketAddrV4::new(Ipv4Addr::LOCALHOST, multicast_port);

        // listener just needs to exist, don't need to use
        let _multicast_listener = UdpSocket::bind(fake_multicast_addr)?;
        let client_addr = SocketAddrV4::new(Ipv4Addr::LOCALHOST, client_port);
        let fake_sender = UdpSocket::bind(client_addr)?;

        fake_sender.set_nonblocking(true)?;
        let client = YeeClient { seeker: fake_sender, multicast_addr: fake_multicast_addr };

        // send mock messages
        let fake_port = 23395;
        let fake_light = UdpSocket::bind(SocketAddrV4::new(Ipv4Addr::LOCALHOST, fake_port))?;
        // there are already newlines in the string, so need to add \n
        let fake_msg = "HTTP/1.1 200 OK\r
Cache-Control: max-age=3600\r
Date: \r
Ext: \r
Location: yeelight://192.168.1.123:12345\r
Server: POSIX UPnP/1.0 YGLC/1\r
id: 0x12345abcde\r
model: ceiling3\r
fw_ver: 20\r
support: get_prop set_default set_power toggle set_bright set_scene cron_add cron_get cron_del start_cf stop_cf set_ct_abx set_name set_adjust adjust_bright adjust_ct\r
power: on\r
bright: 40\r
color_mode: 2\r
ct: 3300\r
rgb: 2\r
hue: 4\r
sat: 100\r
name: light_one\r\n";
        fake_light.send_to(fake_msg.as_bytes(), client_addr)?;
        fake_light.send_to(fake_msg.as_bytes(), client_addr)?;
        fake_light.send_to(fake_msg.as_bytes(), client_addr)?;
        fake_light.send_to(fake_msg.as_bytes(), client_addr)?;
        fake_light.send_to(fake_msg.as_bytes(), client_addr)?;
        fake_light.send_to(fake_msg.as_bytes(), client_addr)?;
        fake_light.send_to(fake_msg.as_bytes(), client_addr)?;
        fake_light.send_to(fake_msg.as_bytes(), client_addr)?;
        fake_light.send_to(fake_msg.as_bytes(), client_addr)?;
        drop(fake_light);

        // WHEN
        let result = client.get_response(Duration::from_millis(500));

        // THEN
        assert_eq!(result.len(), 1);

        Ok(())
    }
}


