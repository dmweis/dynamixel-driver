use dynamixel_driver;

fn main() {
    let mut driver = dynamixel_driver::DynamixelDriver::new("COM11").unwrap();
    driver.write_id(5, 1).unwrap();
    driver.write_id(9, 2).unwrap();
}