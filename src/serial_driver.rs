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

    pub(crate) fn param(&self, index: usize) -> Option<u8> {
        match self.params.get(index) {
            Some(val) => Some(*val),
            None => None,
        }
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
        let buffer = src.as_ref();
        if buffer[0] != 0xFF && buffer[1] != 0xFF {
            return Err(SerialPortError::HeaderError.into());
        }
        let id = buffer[2];
        let len = buffer[3] as usize;
        if src.len() < 4 + len {
            return Ok(None);
        }
        let message = src.split_to(4 + len);

        let _ = StatusError::check_error(message[4])?;
        let params = message[5..5 + (len - 2)].to_vec();
        let checksum = calc_checksum(&message[2..5 + (len - 2)]);
        if &checksum != message.last().unwrap() {
            return Err(SerialPortError::ChecksumError.into());
        }

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
    #[should_panic(expected = "input_voltage_error ")]
    fn test_input_voltage_error() {
        let mut payload =
            BytesMut::from(vec![0xFF, 0xFF, 0x01, 0x03, 0b00000001, 0x20, 0xDB].as_slice());
        let mut codec = DynamixelProtocol {};
        let _ = codec.decode(&mut payload).unwrap().unwrap();
    }

    #[test]
    #[should_panic(expected = "angle_limit_error ")]
    fn test_angle_limit_error() {
        let mut payload =
            BytesMut::from(vec![0xFF, 0xFF, 0x01, 0x03, 0b00000010, 0x20, 0xDB].as_slice());
        let mut codec = DynamixelProtocol {};
        let _ = codec.decode(&mut payload).unwrap().unwrap();
    }

    #[test]
    #[should_panic(expected = "overheating_error ")]
    fn test_overheating_error() {
        let mut payload =
            BytesMut::from(vec![0xFF, 0xFF, 0x01, 0x03, 0b00000100, 0x20, 0xDB].as_slice());
        let mut codec = DynamixelProtocol {};
        let _ = codec.decode(&mut payload).unwrap().unwrap();
    }

    #[test]
    #[should_panic(expected = "range_error ")]
    fn test_range_error() {
        let mut payload =
            BytesMut::from(vec![0xFF, 0xFF, 0x01, 0x03, 0b00001000, 0x20, 0xDB].as_slice());
        let mut codec = DynamixelProtocol {};
        let _ = codec.decode(&mut payload).unwrap().unwrap();
    }

    #[test]
    #[should_panic(expected = "checksum_error ")]
    fn test_checksum_error() {
        let mut payload =
            BytesMut::from(vec![0xFF, 0xFF, 0x01, 0x03, 0b00010000, 0x20, 0xDB].as_slice());
        let mut codec = DynamixelProtocol {};
        let _ = codec.decode(&mut payload).unwrap().unwrap();
    }

    #[test]
    #[should_panic(expected = "overload_error ")]
    fn test_overload_error() {
        let mut payload =
            BytesMut::from(vec![0xFF, 0xFF, 0x01, 0x03, 0b00100000, 0x20, 0xDB].as_slice());
        let mut codec = DynamixelProtocol {};
        let _ = codec.decode(&mut payload).unwrap().unwrap();
    }

    #[test]
    #[should_panic(expected = "instruction_error ")]
    fn test_instruction_error() {
        let mut payload =
            BytesMut::from(vec![0xFF, 0xFF, 0x01, 0x03, 0b01000000, 0x20, 0xDB].as_slice());
        let mut codec = DynamixelProtocol {};
        let _ = codec.decode(&mut payload).unwrap().unwrap();
    }
}
