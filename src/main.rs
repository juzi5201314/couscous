use std::path::PathBuf;

#[tokio::main]
async fn main() {
    let args = argh::from_env::<Arguments>();
    couscous::run(args.conf).await.unwrap();
}

/// arguments
#[derive(argh::FromArgs)]
struct Arguments {
    /// configuration file
    #[argh(option, short = 'c')]
    conf: PathBuf,
}
