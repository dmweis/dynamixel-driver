use async_trait::async_trait;
use bytes::{BufMut, BytesMut};
use futures::{SinkExt, StreamExt};
use std::io::Write;
use std::str;
use tokio::time::{timeout, Duration};
use tokio_serial::{SerialPort, SerialPortBuilderExt};
use tokio_util::codec::{Decoder, Encoder};

use crate::instructions::{calc_checksum, DynamixelDriverError, Instruction, Result, StatusError};

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
        self.params
            .first()
            .cloned()
            .ok_or(DynamixelDriverError::DecodingError("Failed unpacking u8"))
    }

    pub(crate) fn as_u16(&self) -> Result<u16> {
        Ok(u16::from_le_bytes([
            *self
                .params
                .first()
                .ok_or(DynamixelDriverError::DecodingError(
                    "Failed unpacking u16 first element",
                ))?,
            *self
                .params
                .get(1)
                .ok_or(DynamixelDriverError::DecodingError(
                    "Failed unpacking u16 second element",
                ))?,
        ]))
    }

    #[cfg(test)]
    pub(crate) fn as_u16_bad(&self) -> Result<u16> {
        let mut res = 0_u16;
        let a = *self
            .params
            .first()
            .ok_or(DynamixelDriverError::DecodingError("two"))? as u16;
        let b = *self
            .params
            .get(1)
            .ok_or(DynamixelDriverError::DecodingError("three"))? as u16;

        res |= b << 8;
        res |= a;
        Ok(res)
    }
}

pub(crate) struct DynamixelProtocol;

impl Decoder for DynamixelProtocol {
    type Item = Status;
    type Error = DynamixelDriverError;

    fn decode(&mut self, src: &mut BytesMut) -> Result<Option<Self::Item>> {
        if src.len() < 4 {
            return Ok(None);
        }

        let id = src[2];
        let len = src[3] as usize;
        if !src.starts_with(&[0xFF, 0xFF]) || len < 2 {
            // discard 1 byte in case we are starting with FF, FF
            let _ = src.split_to(1);
            if let Some(start) = src.windows(2).position(|pos| pos == [0xFF, 0xFF]) {
                let _ = src.split_to(start);
            } else {
                src.clear();
            }
            return Err(DynamixelDriverError::HeaderError);
        }
        if src.len() < 4 + len {
            return Ok(None);
        }

        let checksum = calc_checksum(&src[2..5 + (len - 2)]);
        if checksum != src[3 + len] {
            // discard byte to force a move
            let _ = src.split_to(1);
            return Err(DynamixelDriverError::ChecksumError);
        }
        let message = src.split_to(4 + len);
        StatusError::check_error(message[4])?;
        let params = message[5..5 + (len - 2)].to_vec();

        Ok(Some(Status::new(id, params)))
    }
}

impl Encoder<Instruction> for DynamixelProtocol {
    type Error = DynamixelDriverError;

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
    async fn flush_and_clear(&mut self) -> Result<()>;
}

pub(crate) const TIMEOUT: u64 = 100;

pub struct FramedSerialDriver {
    framed_port: tokio_util::codec::Framed<tokio_serial::SerialStream, DynamixelProtocol>,
}

impl FramedSerialDriver {
    pub fn new(port: &str) -> Result<FramedSerialDriver> {
        let serial_port = tokio_serial::new(port, 1000000)
            .timeout(std::time::Duration::from_millis(TIMEOUT))
            .open_native_async()
            .map_err(|_| DynamixelDriverError::FailedOpeningSerialPort)?;

        Ok(FramedSerialDriver {
            framed_port: DynamixelProtocol.framed(serial_port),
        })
    }

    pub fn with_baud_rate(port: &str, baud_rate: u32) -> Result<FramedSerialDriver> {
        let serial_port = tokio_serial::new(port, baud_rate)
            .timeout(std::time::Duration::from_millis(TIMEOUT))
            .open_native_async()
            .map_err(|_| DynamixelDriverError::FailedOpeningSerialPort)?;

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
            .map_err(|_| DynamixelDriverError::Timeout)?
            .ok_or(DynamixelDriverError::ReadingError)??;
        Ok(response)
    }

    async fn flush_and_clear(&mut self) -> Result<()> {
        self.framed_port.get_mut().flush()?;
        self.framed_port
            .get_mut()
            .clear(tokio_serial::ClearBuffer::All)?;
        // receive and discard any leftover bytes
        _ = self.receive().await;
        Ok(())
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
    fn test_message_seek_and_decode() {
        let mut payload = BytesMut::from(
            vec![0xFF, 0x12, 0x21, 0xFF, 0xFF, 0x01, 0x03, 0x00, 0x20, 0xDB].as_slice(),
        );
        let mut codec = DynamixelProtocol {};
        assert!(codec.decode(&mut payload).is_err());
        let res = codec.decode(&mut payload).unwrap().unwrap();
        assert_eq!(res, Status::new(1, vec![0x20]));
    }

    #[test]
    fn test_message_skip_header_error_and_decode() {
        let mut payload = BytesMut::from(
            vec![
                0xFF, 0x12, 0x21, 0xFF, 0xFF, 0x1, 0x1, 0xFF, 0xFF, 0x01, 0x03, 0x00, 0x20, 0xDB,
            ]
            .as_slice(),
        );
        let mut codec = DynamixelProtocol {};
        assert!(std::matches!(
            codec.decode(&mut payload).unwrap_err(),
            DynamixelDriverError::HeaderError
        ));
        assert!(std::matches!(
            codec.decode(&mut payload).unwrap_err(),
            DynamixelDriverError::HeaderError
        ));
        let res = codec.decode(&mut payload).unwrap().unwrap();
        assert_eq!(res, Status::new(1, vec![0x20]));
    }

    #[test]
    fn test_message_skip_checksum_error_and_decode() {
        let mut payload =
            BytesMut::from(vec![0xFF, 0xFF, 0xFF, 0x04, 0x03, 0x00, 0x20, 0xD8].as_slice());
        let mut codec = DynamixelProtocol {};
        assert!(std::matches!(
            codec.decode(&mut payload).unwrap_err(),
            DynamixelDriverError::ChecksumError
        ));
        let res = codec.decode(&mut payload).unwrap().unwrap();
        assert_eq!(res, Status::new(4, vec![0x20]));
    }

    #[test]
    fn test_input_voltage_error() {
        let mut payload =
            BytesMut::from(vec![0xFF, 0xFF, 0x01, 0x03, 0b00000001, 0x20, 0xDA].as_slice());
        let mut codec = DynamixelProtocol {};
        let err = codec.decode(&mut payload).unwrap_err();
        if let DynamixelDriverError::StatusError(status) = err {
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
        if let DynamixelDriverError::StatusError(status) = err {
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
        if let DynamixelDriverError::StatusError(status) = err {
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
        if let DynamixelDriverError::StatusError(status) = err {
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
        if let DynamixelDriverError::StatusError(status) = err {
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
        if let DynamixelDriverError::StatusError(status) = err {
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
        if let DynamixelDriverError::StatusError(status) = err {
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
