use dynamixel_driver;
use std::time::Duration;
use std::thread::sleep;

fn main() {
    let mut driver = dynamixel_driver::DynamixelDriver::new("COM11").unwrap();
    // driver.write_position(9, 150.0).unwrap();
    // loop {
    //     println!("{}", driver.read_position(9).unwrap());
    // }
    loop {
        driver.write_position(9, 0.0).unwrap();
        loop {
            let pos = driver.read_position(9).unwrap();
            if pos < 1.0 {
                break;
            }
        }
        driver.write_position(9, 300.0).unwrap();
        loop {
            let pos = driver.read_position(9).unwrap();
            if pos > 299.0 {
                break;
            }
        }
    }
}