use dynamixel_driver;

fn main() {
    let mut driver = dynamixel_driver::DynamixelDriver::new("COM11").unwrap();
    loop {
        if let Ok(position) = driver.read_position(9) {
            println!("servo {} has position of {}", 9, position);
        }
    }
}