use reqwest::dns::{Addrs, Name, Resolve, Resolving};
use std::net::{IpAddr, Ipv4Addr, SocketAddr};

pub(super) struct PublicResolver;

impl Resolve for PublicResolver {
    fn resolve(&self, name: Name) -> Resolving {
        Box::pin(async move {
            let addresses: Vec<SocketAddr> =
                tokio::net::lookup_host((name.as_str(), 0)).await?.collect();
            if addresses.is_empty() || addresses.iter().any(|address| !public(address.ip())) {
                return Err(std::io::Error::other("OAuth destination is not public").into());
            }
            let addresses: Addrs = Box::new(addresses.into_iter());
            Ok(addresses)
        })
    }
}

pub(super) fn public(address: IpAddr) -> bool {
    match address {
        IpAddr::V4(address) => public_v4(address),
        IpAddr::V6(address) => {
            let segments: [u16; 8] = address.segments();
            // Only global unicast; exclude transition and documentation ranges.
            segments[0] & 0xe000 == 0x2000
                && segments[0] != 0x2002
                && !(segments[0] == 0x2001 && segments[1] < 0x200)
                && !(segments[0] == 0x2001 && segments[1] == 0xdb8)
                && !(segments[0] == 0x3fff && segments[1] < 0x1000)
        }
    }
}

fn public_v4(address: Ipv4Addr) -> bool {
    let [a, b, _, _]: [u8; 4] = address.octets();
    !address.is_private()
        && !address.is_loopback()
        && !address.is_link_local()
        && !address.is_documentation()
        && a != 0
        && a < 224
        && !(a == 100 && (64..=127).contains(&b))
        && !(a == 198 && (18..=19).contains(&b))
        && !(a == 192 && b == 0)
}
