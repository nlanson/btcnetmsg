// encode.rs
//
// Module implementing the encoding/decoding of encodable structures
//

use std::net::Ipv4Addr;

use crate::{
    msg::{
        data::{
            Message,
            MessagePayload
        },
        header::{
            VariableInteger,
            Magic,
            Command,
            MessageHeader
        },
        network::{
            NetAddr,
            ServicesList,
            VersionMessage,
            VerackMessage
        }
    },
    net::peer::{
        Port
    }
};

/// Trait to encode self into a format acceptable by the Bitcoin P2P network.
pub trait Encode {
    fn net_encode<W>(&self, w: W) -> usize
    where W: std::io::Write;
}

pub trait Decode: Sized {
    fn net_decode<R>(r: R) -> Result<Self, Error> 
    where R: std::io::Read;
}

#[derive(Debug)]
pub enum Error {
    InvalidData
}

/// Macro to encode integers in little endian.
macro_rules! integer_le_encode {
    ($int: ty) => {
        impl Encode for $int {
            fn net_encode<W>(&self, mut w: W) -> usize
            where W: std::io::Write {
                w.write(&self.to_le_bytes()).expect("Failed to write")
            }
        }
    };
}

/// Macro to decode little endian integers
macro_rules! integer_le_decode {
    ($int: ty) => {
        impl Decode for $int {
            fn net_decode<R>(mut r: R) -> Result<$int, Error>
            where
                R: std::io::Read ,
                Self: Sized
            {
                let mut buf = [0; std::mem::size_of::<$int>()];
                r.read_exact(&mut buf).expect("Failed to read");
                
                let mut ret: u64 = 0;
                let mut i = buf.len() - 1;
                loop {
                    ret ^= buf[i] as u64;
                    if i == 0 { break }
                    i-=1;
                    ret = ret << 8; 
                }
                
                Ok(ret as $int)
            }
        }
    }
}

integer_le_encode!(u8);
integer_le_encode!(u16);
integer_le_encode!(u32);
integer_le_encode!(u64);
integer_le_encode!(usize);

integer_le_decode!(u8);
integer_le_decode!(u16);
integer_le_decode!(u32);
integer_le_decode!(u64);
integer_le_decode!(usize);


/// Macro to encode arrays
macro_rules! array_encode {
    ($len: expr) => {
        impl Encode for [u8; $len] {
            fn net_encode<W>(&self, mut w: W) -> usize
            where W: std::io::Write {
                w.write(self).expect("Failed to write")
            }
        }
    };
}

macro_rules! array_decode {
    ($len: expr) => {
        impl Decode for [u8; $len] {
            fn net_decode<R>(mut r: R) -> Result<Self, Error>
            where
                R: std::io::Read ,
                Self: Sized
            {
                let mut buf: [u8; $len] = [0; $len];
                r.read_exact(&mut buf).expect("Failed to read");
                
                Ok(buf)
            }
        }
    }
}

array_encode!(4);
array_encode!(2);

array_decode!(4);
array_decode!(2);


impl Encode for VariableInteger {
    fn net_encode<W>(&self, mut w: W) -> usize
    where W: std::io::Write {
        match self.0 {
            0..=0xFC => {
                (self.0 as u8).net_encode(w)
            },
            0xFD..=0xFFFF => {
                w.write(&[0xFD]).expect("Failed to write");
                (self.0 as u16).net_encode(w);
                3
            },
            0x10000..=0xFFFF_FFFF => {
                w.write(&[0xFE]).expect("Failed to write");
                (self.0 as u32).net_encode(w);
                5
            },
            _ => {
                w.write(&[0xFF]).expect("Failed to write");
                (self.0 as u64).net_encode(w);
                9
            }
        }
    }
}

impl Decode for VariableInteger {
    fn net_decode<R: std::io::Read >(mut r: R) -> Result<Self, Error> {
        let mut buf = [0; 10];
        let len = r.read(&mut buf).expect("Failed to read");

        match len {
            1 => {
                Ok(VariableInteger::from(buf[0]))
            },
            _ => {
                Ok(
                    VariableInteger::from(
                        u64::net_decode(&buf[1..9]).expect("Failed to decode")
                    )
                ) 
            }
        }        
    }
}

impl Encode for Magic {
    fn net_encode<W>(&self, w: W) -> usize
    where W: std::io::Write {
        self.bytes().net_encode(w)
    }
}

impl Decode for Magic {
    fn net_decode<R>(mut r: R) -> Result<Self, Error>
    where R: std::io::Read {
        let mut buf = [0; 4];
        r.read(&mut buf).expect("Failed to read");
        buf.reverse();

        match Magic::from(buf) {
            Magic::Unknown => return Err(Error::InvalidData),
            x => Ok(x)
        }
    }
}

impl Encode for Command {
    fn net_encode<W>(&self, mut w: W) -> usize
    where W: std::io::Write {
        let mut buf: [u8; 12] = [0; 12];
        let cmd_str = self.to_str().as_bytes();
        buf[..cmd_str.len()].copy_from_slice(&cmd_str);
        w.write(&buf).expect("Failed to write")
    }
}

impl Decode for Command {
    fn net_decode<R>(mut r: R) -> Result<Self, Error>
    where R: std::io::Read {
        let mut buf = [0; 12];
        r.read(&mut buf).expect("Failed to read");

        Self::from_str(
        buf
                .iter()
                .take_while(|x| **x != 0x00)
                .map(|c| *c as char)
                .collect::<String>()
        )
    }
}

impl Encode for MessageHeader {
    fn net_encode<W>(&self, mut w: W) -> usize
    where W: std::io::Write {
        self.magic.net_encode(&mut w) +
        self.command.net_encode(&mut w) +
        self.length.net_encode(&mut w) +
        self.checksum.net_encode(&mut w)
    }
}

impl Decode for MessageHeader {
    fn net_decode<R>(mut r: R) -> Result<Self, Error>
    where R: std::io::Read {
        let magic = Magic::net_decode(&mut r).unwrap();
        let command = Command::net_decode(&mut r).unwrap();
        let length: u32 = Decode::net_decode(&mut r).unwrap();
        let checksum: [u8; 4] = Decode::net_decode(&mut r).unwrap();

        Ok(
            Self::new(magic, command, length as usize, checksum)
        )
    }
}

impl Encode for Message {
    fn net_encode<W>(&self, mut w: W) -> usize
    where W: std::io::Write {
        self.header.net_encode(&mut w) +
        self.payload.net_encode(&mut w)
    }
}

impl Decode for Message {
    fn net_decode<R>(mut r: R) -> Result<Self, Error>
    where R: std::io::Read {
        let header: MessageHeader = Decode::net_decode(&mut r)?;

        // Message payload doesn't implement the [`Decode`] trait on it's own as
        // it cannot be decoded without knowledge of the command used in the header.
        // This is becase each command has a different payload structure.
        let payload: MessagePayload = match header.command {
            Command::Version => MessagePayload::from(VersionMessage::net_decode(&mut r)?),
            Command::Verack => MessagePayload::from(VerackMessage::net_decode(&mut r)?)
        };
        
        Ok(
            Message {
                header: Decode::net_decode(&mut r)?,
                payload
            }
        )
    }
}

impl Encode for MessagePayload {
    fn net_encode<W>(&self, w: W) -> usize
    where W: std::io::Write {
        match self {
            MessagePayload::Version(v) => v.net_encode(w),
            MessagePayload::Verack(v) => v.net_encode(w)
        }
    }
}

/// Strings are encoded as var string which is the string bytes with a varint prefixed
impl Encode for String {
    fn net_encode<W>(&self, mut w: W) -> usize
    where W: std::io::Write {
        VariableInteger::from(self.len()).net_encode(&mut w) +
        w.write(self.as_bytes()).expect("Failed to write")
    }
}

impl Encode for Port {
    fn net_encode<W>(&self, w: W) -> usize
    where W: std::io::Write {
        self.0.net_encode(w)
    }
}

impl Encode for Ipv4Addr {
    fn net_encode<W>(&self, mut w: W) -> usize
    where W: std::io::Write {
        // Ipv4 addresses are encoded as an Ipv4 mapped Ipv6 address.
        w.write(&self.to_ipv6_mapped().octets()).expect("Failed to write")
    }
}

impl Encode for ServicesList {
    fn net_encode<W>(&self, w: W) -> usize
    where W: std::io::Write {
        // Collect all the service flags and XOR them up
        let flag: u64 = 
        self
            .get_flags()
            .iter()
            .fold(
                0,
                |acc, num| 
                acc ^ num.value()
            );

        flag.net_encode(w) //always 8 bytes
    }
}

impl Encode for NetAddr {
    fn net_encode<W>(&self, mut w: W) -> usize
    where W: std::io::Write {
        self.services.net_encode(&mut w) +
        self.ip.net_encode(&mut w) +
        self.port.net_encode(&mut w)
    }
}

impl Encode for std::time::SystemTime {
    fn net_encode<W>(&self, w: W) -> usize
    where W: std::io::Write {
        self
            .duration_since(std::time::SystemTime::UNIX_EPOCH)
            .expect("Could not get unix time")
            .as_secs()
            .net_encode(w)
    }
}

impl Encode for VersionMessage {
    fn net_encode<W>(&self, mut w: W) -> usize
    where W: std::io::Write {
        self.version.net_encode(&mut w) +
        self.services.net_encode(&mut w) +
        self.timestamp.net_encode(&mut w) +
        self.addr_recv.net_encode(&mut w) +
        self.addr_from.net_encode(&mut w) +
        self.nonce.net_encode(&mut w) +
        self.agent.net_encode(&mut w) +
        self.start_height.net_encode(&mut w) +
        (self.relay as u8).net_encode(&mut w)
    }
}

impl Decode for VersionMessage {
    fn net_decode<R>(mut r: R) -> Result<Self, Error>
    where R: std::io::Read {
        todo!("Implement decoding for Version Message and associated types...");
    }
}

impl Encode for VerackMessage {
    fn net_encode<W>(&self, _w: W) -> usize
    where W: std::io::Write {
        0
    }
}

impl Decode for VerackMessage {
    fn net_decode<R>(_r: R) -> Result<Self, Error>
    where R: std::io::Read {
        Ok(Self::default())
    }
}



#[cfg(test)]
mod tests {
    use super::*;
    use crate::msg::network::Services;

    #[test]
    fn varint_test() {
        let ints: [u64; 9] = [0x01, 0xFC, 0xFD, 0x1000, 0xFFFF, 0x10000, 0x55555, 0xFFFF_FFFF, 0x1000_0000_0000];
        let lens: [usize; 9] = [1, 1, 3, 3, 3, 5, 5, 5, 9];

        for i in 0..ints.len() {
            let mut enc: Vec<u8> = Vec::new();
            assert_eq!(VariableInteger::from(ints[i]).net_encode(&mut enc), lens[i]);
            assert_eq!(VariableInteger::net_decode(&enc[..]).unwrap(), VariableInteger::from(ints[i]))
        }
    }

    #[test]
    fn network_magic() {
        let mut main: Vec<u8> = Vec::new();
        let mut test: Vec<u8> = Vec::new();

        Magic::Main.net_encode(&mut main);
        Magic::Test.net_encode(&mut test);

        assert_eq!(main, [0xF9, 0xBE, 0xB4, 0xD9]);
        assert_eq!(test, [0xFA, 0xBF, 0xB5, 0xDA]);
    }

    #[test]
    fn service_flags() {
        let mut flags = ServicesList::new();
        flags.add_flag(Services::Network);
        
        let mut encoded = Vec::new();
        flags.net_encode(&mut encoded);
        
        assert_eq!(encoded, &[0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00])
    }

    #[test]
    fn integer_le() {
        let int: u8 = 0xFF;
        let mut enc: Vec<u8> = Vec::new();
        int.net_encode(&mut enc);
        let dec = u8::net_decode(&enc[..]).expect("Failed to decode");
        assert_eq!(int, dec);

        let int: u16 = 0xFFFF;
        let mut enc: Vec<u8> = Vec::new();
        int.net_encode(&mut enc);
        let dec = u16::net_decode(&enc[..]).expect("Failed to decode");
        assert_eq!(int, dec);

        let int: u32 = 0xFFFF_FFFF;
        let mut enc: Vec<u8> = Vec::new();
        int.net_encode(&mut enc);
        let dec = u32::net_decode(&enc[..]).expect("Failed to decode");
        assert_eq!(int, dec);

        let int: u64 = 0xFFFF_FFFF_FFFF_FFFF;
        let mut enc: Vec<u8> = Vec::new();
        int.net_encode(&mut enc);
        let dec = u64::net_decode(&enc[..]).expect("Failed to decode");
        assert_eq!(int, dec);
    }

    #[test]
    fn header_decode() {
        let header = MessageHeader::new(Magic::Main, Command::Verack, 00, [0x5D, 0xF6, 0xE0, 0xE2]);
        let mut enc: Vec<u8> = Vec::new();
        header.net_encode(&mut enc);
        let dec: MessageHeader = Decode::net_decode(&enc[..]).expect("Failed to decode");
        assert_eq!(header, dec);
    }
}