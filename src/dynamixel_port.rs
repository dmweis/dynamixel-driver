use std::error::Error;
use std::time::Duration;
use std::iter::{repeat, FromIterator};
use std::io::{Read, Write};

use serialport;
use serialport::SerialPort;

pub(crate) trait Instruction {
    fn serialize(&self) -> Vec<u8>;
}

struct ReadInstruction {
    id: u8,
    addr: u8,
    length: u8,
}

pub(crate) fn calc_checksum(payload: &[u8]) -> u8 {
    let mut sum: u8 = 0;
    for b in payload {
        sum = sum.wrapping_add(*b);
    }
    !sum
}

impl ReadInstruction {
    fn new(id: u8, addr: u8, length: u8) -> ReadInstruction {
        ReadInstruction {
            id,
            addr,
            length,
        }
    }
}

impl Instruction for ReadInstruction {
    fn serialize(&self) -> Vec<u8> {
        let mut data = vec![
            0xFF, // header
            0xFF,
            self.id, // ID
            0x04, // Len
            0x02, // Instruction
            self.addr,
            self.length,
        ];
        let checksum = calc_checksum(&data[2..]);
        data.push(checksum);
        data
    }
}

#[derive(PartialEq, Debug)]
enum StatusError {
    InstructionError,
    OverloadError,
    ChecksumError,
    RangeError,
    OverheatingError,
    AngleLimitError,
    InputVoltageError,
}

impl StatusError {
    fn check_error(data: u8) -> Option<Vec<StatusError>> {
        if data & (1<<7) != 0 {
            return None
        }
        let mut errors = vec![];
        if data & (1<<0) != 0 {
            errors.push(StatusError::InputVoltageError);
        }
        if data & (1<<1) != 0 {
            errors.push(StatusError::AngleLimitError);
        }
        if data & (1<<2) != 0 {
            errors.push(StatusError::OverheatingError);
        }
        if data & (1<<3) != 0 {
            errors.push(StatusError::RangeError);
        }
        if data & (1<<4) != 0 {
            errors.push(StatusError::ChecksumError);
        }
        if data & (1<<5) != 0 {
            errors.push(StatusError::OverloadError);
        }
        if data & (1<<6) != 0 {
            errors.push(StatusError::InstructionError);
        }
        Some(errors)
    }
}

#[derive(Debug)]
pub(crate) struct Status {
    id: u8,
    error: Option<Vec<StatusError>>,
    params: Vec<u8>
}

impl Status {
    fn load(data: &[u8]) -> Result<Status, Box<dyn Error>> {
        if data.len() < 6 {
            Err("Packet too small")?
        }
        if data[0] != 0xFF && data[1] != 0xFF {
            Err("Header parsing error")?;
        }
        let id = data[2];
        let len = data[3] - 2;
        let error = StatusError::check_error(data[4]);
        let params = Vec::from_iter(data[5..5+(len as usize)].iter().cloned());
        let checksum = calc_checksum(&data[2..5+(len as usize)]);
        if &checksum != data.last().unwrap() {
            Err("Checksum error")?
        }
        Ok(Status{
            id,
            error,
            params,
        })
    }
}

pub(crate) struct DynamixelPort {
    port: Box<dyn SerialPort>
}

impl DynamixelPort {
    pub(crate) fn new(port_name: &str) -> Result<DynamixelPort, Box<dyn Error>> {
        let mut port = serialport::open(&port_name)?;
        port.set_baud_rate(1000000)?;
        port.set_timeout(Duration::from_millis(100))?;
        Ok(DynamixelPort {
            port
        })
    }

    pub(crate) fn read_u8(&mut self, id: u8, addr: u8) -> Result<u8, Box<dyn Error>> {
        let command = ReadInstruction::new(id, addr, 1);
        self.write_message(command)?;
        let response = self.read_message()?;
        Ok(response.params[0])
    }

    pub(crate) fn read_u16(&mut self, id: u8, addr: u8) -> Result<u16, Box<dyn Error>> {
        let command = ReadInstruction::new(id, addr, 2);
        self.write_message(command)?;
        let response = self.read_message()?;
        let mut res = 0_u16;
        let a = response.params[0] as u16;
        let b = response.params[1] as u16;
        res |= a << 8;
        res |= b;
        Ok(res)
    }

    pub(crate) fn write_message(&mut self, message: impl Instruction) -> Result<(), Box<dyn Error>> {
        let payload = message.serialize();
        self.port.write(&payload)?;
        Ok(())
    }

    pub(crate) fn read_message(&mut self) -> Result<Status, Box<dyn Error>> {
        let mut buffer = [0; 4];
        self.port.read_exact(&mut buffer)?;
        if buffer[0] != 0xFF && buffer[1] != 0xFF {
            Err("Invalid header")?
        }
        let len = buffer[3] as usize;
        let mut data = Vec::with_capacity(len + 4);
        data.extend_from_slice(&buffer);
        data.extend(repeat(0).take(len));
        self.port.read_exact(&mut data[4..4+len])?;
        let status = Status::load(&data)?;
        Ok(status)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn simple_status_loading() {
        let status = Status::load(&[
            0xFF,
            0xFF,
            0x01,
            0x02,
            0x24,
            0xD8
        ]).unwrap();
        assert_eq!(status.id, 1);
        assert!(&status.error == &Some(vec![
            StatusError::OverloadError,
            StatusError::OverheatingError,
        ]) || &status.error == &Some(vec![
            StatusError::OverheatingError,
            StatusError::OverloadError,
        ]))
    }

    #[test]
    fn read_instruction_serialization() {
        let read = ReadInstruction::new(1, 43, 1);
        let payload = read.serialize();
        let expected = vec![0xFF_u8, 0xFF, 0x01, 0x04, 0x02, 0x2B, 0x01, 0xCC];
        assert_eq!(payload, expected);
    }
}
