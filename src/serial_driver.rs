use async_trait::async_trait;
use bytes::{BufMut, BytesMut};
use futures::{SinkExt, StreamExt};
use std::str;
use thiserror::Error;
use tokio::time::{timeout, Duration};
use tokio_util::codec::{Decoder, Encoder};

use anyhow::{self, Result};

use crate::instructions::{calc_checksum, Instruction, StatusError};

#[derive(Error, Debug)]
#[non_exhaustive]
pub(crate) enum SerialPortError {
    #[error("connection timeout")]
    Timeout,
    #[error("{0}")]
    StatusError(StatusError),
    #[error("checksum error on arriving packet")]
    ChecksumError,
    #[error("invalid header")]
    HeaderError,
    #[error("reading error")]
    ReadingError,
    #[error("decoding error for {0}")]
    DecodingError(&'static str),
    #[error("Id mismatch error. Expected {0} got {1}")]
    IdMismatchError(u8, u8),
}

#[derive(PartialEq, Debug)]
pub(crate) struct Status {
    id: u8,
    params: Vec<u8>,
}

impl Status {
    pub(crate) fn new(id: u8, params: Vec<u8>) -> Status {
        Status { id, params }
    }

    pub fn id(&self) -> u8 {
        self.id
    }

    pub(crate) fn as_u8(&self) -> Result<u8> {
        Ok(self
            .params
            .get(0)
            .cloned()
            .ok_or(SerialPortError::DecodingError("Failed unpacking u8"))?)
    }

    pub(crate) fn as_u16(&self) -> Result<u16> {
        Ok(u16::from_le_bytes([
            *self.params.get(0).ok_or(SerialPortError::DecodingError(
                "Failed unpacking u16 first element",
            ))?,
            *self.params.get(1).ok_or(SerialPortError::DecodingError(
                "Failed unpacking u16 second element",
            ))?,
        ]))
    }

    #[cfg(test)]
    pub(crate) fn as_u16_bad(&self) -> Result<u16> {
        let mut res = 0_u16;
        let a = *self
            .params
            .get(0)
            .ok_or(SerialPortError::DecodingError("two"))? as u16;
        let b = *self
            .params
            .get(1)
            .ok_or(SerialPortError::DecodingError("three"))? as u16;

        res |= b << 8;
        res |= a;
        Ok(res)
    }
}

pub(crate) struct DynamixelProtocol;

impl Decoder for DynamixelProtocol {
    type Item = Status;
    type Error = anyhow::Error;

    fn decode(&mut self, src: &mut BytesMut) -> Result<Option<Self::Item>> {
        if src.len() < 4 {
            return Ok(None);
        }

        let id = src[2];
        let len = src[3] as usize;
        if !src.starts_with(&[0xFF, 0xFF]) || len == 0 {
            if let Some(start) = src.windows(2).position(|pos| pos == [0xFF, 0xFF]) {
                let _ = src.split_to(start);
            } else {
                src.clear();
            }
            return Err(SerialPortError::HeaderError.into());
        }
        if src.len() < 4 + len {
            return Ok(None);
        }
        let message = src.split_to(4 + len);

        let checksum = calc_checksum(&message[2..5 + (len - 2)]);
        if &checksum != message.last().unwrap() {
            return Err(SerialPortError::ChecksumError.into());
        }
        let _ = StatusError::check_error(message[4])?;
        let params = message[5..5 + (len - 2)].to_vec();

        Ok(Some(Status::new(id, params)))
    }
}

impl Encoder<Instruction> for DynamixelProtocol {
    type Error = anyhow::Error;

    fn encode(&mut self, data: Instruction, buf: &mut BytesMut) -> Result<()> {
        let msg = data.serialize();
        buf.reserve(msg.len());
        buf.put(msg.as_ref());
        Ok(())
    }
}

#[async_trait]
pub(crate) trait FramedDriver: Send + Sync {
    async fn send(&mut self, instruction: Instruction) -> Result<()>;
    async fn receive(&mut self) -> Result<Status>;
}

pub(crate) const TIMEOUT: u64 = 100;

pub struct FramedSerialDriver {
    framed_port: tokio_util::codec::Framed<tokio_serial::Serial, DynamixelProtocol>,
}

impl FramedSerialDriver {
    pub fn new(port: &str) -> Result<FramedSerialDriver> {
        let mut settings = tokio_serial::SerialPortSettings::default();
        settings.baud_rate = 1000000;
        settings.timeout = std::time::Duration::from_millis(TIMEOUT);
        let serial_port = tokio_serial::Serial::from_path(port, &settings)?;
        Ok(FramedSerialDriver {
            framed_port: DynamixelProtocol.framed(serial_port),
        })
    }

    pub fn with_baud_rate(port: &str, baud_rate: u32) -> Result<FramedSerialDriver> {
        let mut settings = tokio_serial::SerialPortSettings::default();
        settings.baud_rate = baud_rate;
        settings.timeout = std::time::Duration::from_millis(TIMEOUT);
        let serial_port = tokio_serial::Serial::from_path(port, &settings)?;
        Ok(FramedSerialDriver {
            framed_port: DynamixelProtocol.framed(serial_port),
        })
    }
}

#[async_trait]
impl FramedDriver for FramedSerialDriver {
    async fn send(&mut self, instruction: Instruction) -> Result<()> {
        self.framed_port.send(instruction).await?;
        Ok(())
    }

    async fn receive(&mut self) -> Result<Status> {
        let response = timeout(Duration::from_millis(TIMEOUT), self.framed_port.next())
            .await
            .map_err(|_| SerialPortError::Timeout)?
            .ok_or(SerialPortError::ReadingError)??;
        Ok(response)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bytes::BytesMut;

    #[test]
    fn test_message_decode() {
        let mut payload = BytesMut::from(vec![0xFF, 0xFF, 0x01, 0x03, 0x00, 0x20, 0xDB].as_slice());
        let mut codec = DynamixelProtocol {};
        let res = codec.decode(&mut payload).unwrap().unwrap();
        assert_eq!(res, Status::new(1, vec![0x20]));
    }

    #[test]
    fn test_input_voltage_error() {
        let mut payload =
            BytesMut::from(vec![0xFF, 0xFF, 0x01, 0x03, 0b00000001, 0x20, 0xDA].as_slice());
        let mut codec = DynamixelProtocol {};
        let err = codec.decode(&mut payload).unwrap_err();
        let status = err.downcast::<SerialPortError>().unwrap();
        if let SerialPortError::StatusError(status) = status {
            assert!(status.input_voltage_error);
        } else {
            panic!();
        }
    }

    #[test]
    fn test_angle_limit_error() {
        let mut payload =
            BytesMut::from(vec![0xFF, 0xFF, 0x01, 0x03, 0b00000010, 0x20, 0xD9].as_slice());
        let mut codec = DynamixelProtocol {};
        let err = codec.decode(&mut payload).unwrap_err();
        let status = err.downcast::<SerialPortError>().unwrap();
        if let SerialPortError::StatusError(status) = status {
            assert!(status.angle_limit_error);
        } else {
            panic!();
        }
    }

    #[test]
    fn test_overheating_error() {
        let mut payload =
            BytesMut::from(vec![0xFF, 0xFF, 0x01, 0x03, 0b00000100, 0x20, 0xD7].as_slice());
        let mut codec = DynamixelProtocol {};
        let err = codec.decode(&mut payload).unwrap_err();
        let status = err.downcast::<SerialPortError>().unwrap();
        if let SerialPortError::StatusError(status) = status {
            assert!(status.overheating_error);
        } else {
            panic!();
        }
    }

    #[test]
    fn test_range_error() {
        let mut payload =
            BytesMut::from(vec![0xFF, 0xFF, 0x01, 0x03, 0b00001000, 0x20, 0xD3].as_slice());
        let mut codec = DynamixelProtocol {};
        let err = codec.decode(&mut payload).unwrap_err();
        let status = err.downcast::<SerialPortError>().unwrap();
        if let SerialPortError::StatusError(status) = status {
            assert!(status.range_error);
        } else {
            panic!();
        }
    }

    #[test]
    fn test_checksum_error() {
        let mut payload =
            BytesMut::from(vec![0xFF, 0xFF, 0x01, 0x03, 0b00010000, 0x20, 0xCB].as_slice());
        let mut codec = DynamixelProtocol {};
        let err = codec.decode(&mut payload).unwrap_err();
        let status = err.downcast::<SerialPortError>().unwrap();
        if let SerialPortError::StatusError(status) = status {
            assert!(status.checksum_error);
        } else {
            panic!();
        }
    }

    #[test]
    fn test_overload_error() {
        let mut payload =
            BytesMut::from(vec![0xFF, 0xFF, 0x01, 0x03, 0b00100000, 0x20, 0xBB].as_slice());
        let mut codec = DynamixelProtocol {};
        let err = codec.decode(&mut payload).unwrap_err();
        let status = err.downcast::<SerialPortError>().unwrap();
        if let SerialPortError::StatusError(status) = status {
            assert!(status.overload_error);
        } else {
            panic!();
        }
    }

    #[test]
    fn test_instruction_error() {
        let mut payload =
            BytesMut::from(vec![0xFF, 0xFF, 0x01, 0x03, 0b01000000, 0x20, 0x9B].as_slice());
        let mut codec = DynamixelProtocol {};
        let err = codec.decode(&mut payload).unwrap_err();
        let status = err.downcast::<SerialPortError>().unwrap();
        if let SerialPortError::StatusError(status) = status {
            assert!(status.instruction_error);
        } else {
            panic!();
        }
    }

    #[test]
    fn endianness_test() {
        let a = Status::new(0, vec![10, 20]);
        assert_eq!(a.as_u16().unwrap(), a.as_u16_bad().unwrap());
    }
}
