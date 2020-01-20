use std::error::Error;
use std::time::Duration;
use std::iter::{repeat, FromIterator};
use std::io::{Read, Write};

use serialport;
use serialport::SerialPort;

pub(crate) trait Instruction {
    fn serialize(&self) -> Vec<u8>;
}

pub(crate) fn calc_checksum(payload: &[u8]) -> u8 {
    let mut sum: u8 = 0;
    for b in payload {
        sum = sum.wrapping_add(*b);
    }
    !sum
}

struct ReadInstruction {
    id: u8,
    addr: u8,
    length: u8,
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

pub(crate) struct WriteInstruction {
    id: u8,
    addr: u8,
    payload: Vec<u8>,
}

impl WriteInstruction {
    pub(crate) fn with_u8(id: u8, addr: u8, data: u8) -> WriteInstruction {
        WriteInstruction {
            id,
            addr,
            payload: vec![data],
        }
    }

    pub(crate) fn with_u16(id: u8, addr: u8, data: u16) -> WriteInstruction {
        let a = (data >> 8) as u8;
        let b = data as u8;
        WriteInstruction {
            id,
            addr,
            payload: vec![b, a],
        }
    }
}

impl Instruction for WriteInstruction {
    fn serialize(&self) -> Vec<u8> {
        let len = (self.payload.len() + 3) as u8;
        let mut data = vec![
            0xFF, // header
            0xFF,
            self.id, // ID
            len, // Length
            0x03, // Instruction
            self.addr,
        ];
        data.extend(self.payload.iter());
        let checksum = calc_checksum(&data[2..]);
        data.push(checksum);
        data
    }
}

#[derive(PartialEq, Debug)]
struct StatusError {
    instruction_error: bool,
    overload_error: bool,
    checksum_error: bool,
    range_error: bool,
    overheating_error: bool,
    angle_limit_error: bool,
    input_voltage_error: bool,
}

impl StatusError {
    fn check_error(flag: u8) -> Result<(), Box<StatusError>> {
        if flag & (1<<7) != 0 {
            return Ok(())
        }
        let status_error = StatusError {
            input_voltage_error: flag & (1<<0) != 0,
            angle_limit_error: flag & (1<<1) != 0,
            overheating_error: flag & (1<<2) != 0,
            range_error: flag & (1<<3) != 0,
            checksum_error: flag & (1<<4) != 0,
            overload_error: flag & (1<<5) != 0,
            instruction_error: flag & (1<<6) != 0,
        };
        Err(Box::new(status_error))
    }
}

impl Error for StatusError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        None
    }
}

impl std::fmt::Display for StatusError {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        let mut description = String::new();
        if self.input_voltage_error {
            description.push_str("input_voltage_error ");
        }
        if self.angle_limit_error {
            description.push_str("angle_limit_error ");
        }
        if self.overheating_error {
            description.push_str("overheating_error ");
        }
        if self.range_error {
            description.push_str("range_error ");
        }
        if self.checksum_error {
            description.push_str("checksum_error ");
        }
        if self.overload_error {
            description.push_str("overload_error ");
        }
        if self.instruction_error {
            description.push_str("instruction_error ");
        }
        write!(f, "{}", description)
    }
}

#[derive(Debug)]
pub(crate) struct Status {
    id: u8,
    // error: Result<StatusError>,
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
        StatusError::check_error(data[4])?;
        let params = Vec::from_iter(data[5..5+(len as usize)].iter().cloned());
        let checksum = calc_checksum(&data[2..5+(len as usize)]);
        if &checksum != data.last().unwrap() {
            Err("Checksum error")?
        }
        Ok(Status{
            id,
            // error,
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
        res |= b << 8;
        res |= a;
        Ok(res)
    }

    pub(crate) fn write_u16(&mut self, id: u8, addr: u8, value: u16) -> Result<(), Box<dyn Error>> {
        let msg = WriteInstruction::with_u16(id, addr, value);
        self.write_message(msg)?;
        let _response = self.read_message()?;
        Ok(())
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

        // if status.error.unwrap().
        // if let Some(errors) = status.error {
        //     errors?;
        // }
        Ok(status)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[should_panic(expected = "overload_error: true, overheating_error: true")]
    // #[should_panic(expected = "overload_error: true")]
    fn simple_status_loading() {
        let _error = Status::load(&[
            0xFF, // header
            0xFF,
            0x01, // id
            0x02, // instruction
            0x24,
            0xD8
        ]).unwrap();
    }

    #[test]
    fn read_instruction_serialization() {
        let read = ReadInstruction::new(1, 43, 1);
        let payload = read.serialize();
        let expected = vec![0xFF_u8, 0xFF, 0x01, 0x04, 0x02, 0x2B, 0x01, 0xCC];
        assert_eq!(payload, expected);
    }

    #[test]
    fn write_instruction_serialization_u8() {
        let write = WriteInstruction::with_u8(0xFE, 0x03, 1);
        let payload = write.serialize();
        let expected = vec![0xFF, 0xFF, 0xFE, 0x04, 0x03, 0x03, 0x01, 0xF6];
        assert_eq!(payload, expected);
    }
}
