use clap::Clap;

#[derive(Clap)]
#[clap()]
pub struct Args {
    #[clap(about = "Serial port to use")]
    pub port: String,
}
