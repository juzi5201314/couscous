use std::path::PathBuf;

#[tokio::main]
async fn main() {
    let args = argh::from_env::<Arguments>();

    match args.sub {
        SubCommand::Run(args) => {
            couscous::run(args.conf).await.unwrap();
        }
        SubCommand::RcGen(args) => {
            let out = args.out_dir.unwrap_or_else(|| PathBuf::from("."));
            let cert = rcgen::generate_simple_self_signed(args.hosts).unwrap();
            std::fs::write(out.join("cert.pem"), cert.serialize_pem().unwrap()).unwrap();
            std::fs::write(out.join("key.pem"), cert.serialize_private_key_pem()).unwrap();
        }
    }
}

/// arguments
#[derive(argh::FromArgs)]
struct Arguments {
    #[argh(subcommand)]
    sub: SubCommand,
}

#[derive(argh::FromArgs)]
#[argh(subcommand)]
enum SubCommand {
    Run(Run),
    RcGen(RcGen),
}

#[derive(argh::FromArgs)]
#[argh(subcommand, name = "run")]
/// run couscous
struct Run {
    /// configuration file
    #[argh(option, short = 'c')]
    conf: PathBuf,
}

#[derive(argh::FromArgs)]
#[argh(subcommand, name = "rcgen")]
/// gen self signed certificate
struct RcGen {
    #[argh(positional)]
    /// hosts
    hosts: Vec<String>,

    #[argh(option, short = 'o')]
    /// out dir
    out_dir: Option<PathBuf>,
}
