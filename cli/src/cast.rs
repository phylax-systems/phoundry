pub mod cmd;

mod utils;

use cast::{Cast, SimpleCast};

mod opts;
use opts::{
    cast::{Opts, Subcommands, WalletSubcommands},
    EthereumOpts, WalletType,
};

use ethers::{
    core::{
        rand::thread_rng,
        types::{BlockId, BlockNumber::Latest},
    },
    providers::{Middleware, Provider},
    signers::{LocalWallet, Signer},
    types::{Address, NameOrAddress, Signature, U256},
};
use rayon::prelude::*;
use regex::RegexSet;
use rustc_hex::ToHex;
use std::{
    convert::TryFrom,
    io::{self, Write},
    str::FromStr,
    time::Instant,
};
use structopt::StructOpt;

use crate::utils::read_secret;

#[tokio::main]
async fn main() -> eyre::Result<()> {
    color_eyre::install()?;

    let opts = Opts::from_args();
    match opts.sub {
        Subcommands::MaxInt => {
            println!("{}", SimpleCast::max_int()?);
        }
        Subcommands::MinInt => {
            println!("{}", SimpleCast::min_int()?);
        }
        Subcommands::MaxUint => {
            println!("{}", SimpleCast::max_uint()?);
        }
        Subcommands::FromUtf8 { text } => {
            let val = unwrap_or_stdin(text)?;
            println!("{}", SimpleCast::from_utf8(&val));
        }
        Subcommands::ToHex { decimal } => {
            let val = unwrap_or_stdin(decimal)?;
            println!("{}", SimpleCast::hex(U256::from_dec_str(&val)?));
        }
        Subcommands::ToHexdata { input } => {
            let val = unwrap_or_stdin(input)?;
            let output = match val {
                s if s.starts_with('@') => {
                    let var = std::env::var(&s[1..])?;
                    var.as_bytes().to_hex()
                }
                s if s.starts_with('/') => {
                    let input = std::fs::read(s)?;
                    input.to_hex()
                }
                s => {
                    let mut output = String::new();
                    for s in s.split(':') {
                        output.push_str(&s.trim_start_matches("0x").to_lowercase())
                    }
                    output
                }
            };
            println!("0x{}", output);
        }
        Subcommands::ToCheckSumAddress { address } => {
            let val = unwrap_or_stdin(address)?;
            println!("{}", SimpleCast::checksum_address(&val)?);
        }
        Subcommands::ToAscii { hexdata } => {
            let val = unwrap_or_stdin(hexdata)?;
            println!("{}", SimpleCast::ascii(&val)?);
        }
        Subcommands::ToBytes32 { bytes } => {
            let val = unwrap_or_stdin(bytes)?;
            println!("{}", SimpleCast::bytes32(&val)?);
        }
        Subcommands::ToDec { hexvalue } => {
            let val = unwrap_or_stdin(hexvalue)?;
            println!("{}", SimpleCast::to_dec(&val)?);
        }
        Subcommands::ToFix { decimals, value } => {
            let val = unwrap_or_stdin(value)?;
            println!(
                "{}",
                SimpleCast::to_fix(unwrap_or_stdin(decimals)?, U256::from_dec_str(&val)?)?
            );
        }
        Subcommands::ToUint256 { value } => {
            let val = unwrap_or_stdin(value)?;
            println!("{}", SimpleCast::to_uint256(&val)?);
        }
        Subcommands::ToWei { value, unit } => {
            let val = unwrap_or_stdin(value)?;
            println!(
                "{}",
                SimpleCast::to_wei(
                    U256::from_dec_str(&val)?,
                    unit.unwrap_or_else(|| String::from("eth"))
                )?
            );
        }
        Subcommands::FromWei { value, unit } => {
            let val = unwrap_or_stdin(value)?;
            println!(
                "{}",
                SimpleCast::from_wei(
                    U256::from_dec_str(&val)?,
                    unit.unwrap_or_else(|| String::from("eth"))
                )?
            );
        }
        Subcommands::Block { rpc_url, block, full, field, to_json } => {
            let provider = Provider::try_from(rpc_url)?;
            println!("{}", Cast::new(provider).block(block, full, field, to_json).await?);
        }
        Subcommands::BlockNumber { rpc_url } => {
            let provider = Provider::try_from(rpc_url)?;
            println!("{}", Cast::new(provider).block_number().await?);
        }
        Subcommands::Call { rpc_url, address, sig, args } => {
            let provider = Provider::try_from(rpc_url)?;
            println!("{}", Cast::new(provider).call(address, &sig, args).await?);
        }
        Subcommands::Calldata { sig, args } => {
            println!("{}", SimpleCast::calldata(sig, &args)?);
        }
        Subcommands::Chain { rpc_url } => {
            let provider = Provider::try_from(rpc_url)?;
            println!("{}", Cast::new(provider).chain().await?);
        }
        Subcommands::ChainId { rpc_url } => {
            let provider = Provider::try_from(rpc_url)?;
            println!("{}", Cast::new(provider).chain_id().await?);
        }
        Subcommands::Code { block, who, rpc_url } => {
            let provider = Provider::try_from(rpc_url)?;
            println!("{}", Cast::new(provider).code(who, block).await?);
        }
        Subcommands::Namehash { name } => {
            println!("{}", SimpleCast::namehash(&name)?);
        }
        Subcommands::Tx { rpc_url, hash, field, to_json } => {
            let provider = Provider::try_from(rpc_url)?;
            println!("{}", Cast::new(&provider).transaction(hash, field, to_json).await?)
        }
        Subcommands::SendTx { eth, to, sig, cast_async, args } => {
            let provider = Provider::try_from(eth.rpc_url.as_str())?;
            let chain_id = Cast::new(&provider).chain_id().await?;

            if let Some(signer) = eth.signer_with(chain_id, provider.clone()).await? {
                match signer {
                    WalletType::Ledger(signer) => {
                        cast_send(&signer, signer.address(), to, sig, args, cast_async).await?;
                    }
                    WalletType::Local(signer) => {
                        cast_send(&signer, signer.address(), to, sig, args, cast_async).await?;
                    }
                    WalletType::Trezor(signer) => {
                        cast_send(&signer, signer.address(), to, sig, args, cast_async).await?;
                    }
                }
            } else {
                let from = eth.from.expect("No ETH_FROM or signer specified");
                cast_send(provider, from, to, sig, args, cast_async).await?;
            }
        }
        Subcommands::Estimate { eth, to, sig, args } => {
            let provider = Provider::try_from(eth.rpc_url.as_str())?;
            let cast = Cast::new(&provider);
            // chain id does not matter here, we're just trying to get the address
            let from = if let Some(signer) = eth.signer(0.into()).await? {
                match signer {
                    WalletType::Ledger(signer) => signer.address(),
                    WalletType::Local(signer) => signer.address(),
                    WalletType::Trezor(signer) => signer.address(),
                }
            } else {
                eth.from.expect("No ETH_FROM or signer specified")
            };
            let gas = cast.estimate(from, to, Some((sig.as_str(), args))).await?;
            println!("{}", gas);
        }
        Subcommands::CalldataDecode { sig, calldata } => {
            let tokens = SimpleCast::abi_decode(&sig, &calldata, true)?;
            let tokens = foundry_utils::format_tokens(&tokens);
            tokens.for_each(|t| println!("{}", t));
        }
        Subcommands::AbiDecode { sig, calldata, input } => {
            let tokens = SimpleCast::abi_decode(&sig, &calldata, input)?;
            let tokens = foundry_utils::format_tokens(&tokens);
            tokens.for_each(|t| println!("{}", t));
        }
        Subcommands::FourByte { selector } => {
            let sigs = foundry_utils::fourbyte(&selector).await?;
            sigs.iter().for_each(|sig| println!("{}", sig.0));
        }
        Subcommands::FourByteDecode { calldata, id } => {
            let sigs = foundry_utils::fourbyte_possible_sigs(&calldata, id).await?;
            sigs.iter().enumerate().for_each(|(i, sig)| println!("{}) \"{}\"", i + 1, sig));

            let sig = match sigs.len() {
                0 => Err(eyre::eyre!("No signatures found")),
                1 => Ok(sigs.get(0).unwrap()),
                _ => {
                    print!("Select a function signature by number: ");
                    io::stdout().flush()?;
                    let mut input = String::new();
                    io::stdin().read_line(&mut input)?;
                    let i: usize = input.trim().parse()?;
                    Ok(sigs.get(i - 1).expect("Invalid signature index"))
                }
            }?;

            let tokens = SimpleCast::abi_decode(sig, &calldata, true)?;
            let tokens = foundry_utils::format_tokens(&tokens);

            tokens.for_each(|t| println!("{}", t));
        }
        Subcommands::Age { block, rpc_url } => {
            let provider = Provider::try_from(rpc_url)?;
            println!(
                "{}",
                Cast::new(provider).age(block.unwrap_or(BlockId::Number(Latest))).await?
            );
        }
        Subcommands::Balance { block, who, rpc_url } => {
            let provider = Provider::try_from(rpc_url)?;
            println!("{}", Cast::new(provider).balance(who, block).await?);
        }
        Subcommands::BaseFee { block, rpc_url } => {
            let provider = Provider::try_from(rpc_url)?;
            println!(
                "{}",
                Cast::new(provider).base_fee(block.unwrap_or(BlockId::Number(Latest))).await?
            );
        }
        Subcommands::GasPrice { rpc_url } => {
            let provider = Provider::try_from(rpc_url)?;
            println!("{}", Cast::new(provider).gas_price().await?);
        }
        Subcommands::Keccak { data } => {
            println!("{}", SimpleCast::keccak(&data)?);
        }

        Subcommands::Interface { contract_address, output_location } => {
            println!("Generating interfaces from etherscan's ABI..");
            let interfaces =
                SimpleCast::generate_interface(unwrap_or_stdin(contract_address).unwrap()).await?;
            match output_location {
                Some(loc) => {
                    std::fs::create_dir_all(output_location);
                    for interface in interfaces {
                        std::fs::write(
                            format!("./{}/{}.sol", loc, interface.name),
                            interface.source,
                        )?;
                        println!("Saved interface at ./{}/{}.sol", loc, interface.name);
                    }
                }
                None => {
                    for interface in interfaces {
                        println!(
                            "Interface for contract {}: \n {}",
                            interface.name, interface.source
                        )
                    }
                }
            }
        }
        Subcommands::ResolveName { who, rpc_url, verify } => {
            let provider = Provider::try_from(rpc_url)?;
            let who = unwrap_or_stdin(who)?;
            let address = provider.resolve_name(&who).await?;
            if verify {
                let name = provider.lookup_address(address).await?;
                assert_eq!(
                    name, who,
                    "forward lookup verification failed. got {}, expected {}",
                    name, who
                );
            }
            println!("{:?}", address);
        }
        Subcommands::LookupAddress { who, rpc_url, verify } => {
            let provider = Provider::try_from(rpc_url)?;
            let who = unwrap_or_stdin(who)?;
            let name = provider.lookup_address(who).await?;
            if verify {
                let address = provider.resolve_name(&name).await?;
                assert_eq!(
                    address, who,
                    "forward lookup verification failed. got {}, expected {}",
                    name, who
                );
            }
            println!("{}", name);
        }
        Subcommands::Storage { address, slot, rpc_url, block } => {
            let provider = Provider::try_from(rpc_url)?;
            let value = provider.get_storage_at(address, slot, block).await?;
            println!("{:?}", value);
        }
        Subcommands::Nonce { block, who, rpc_url } => {
            let provider = Provider::try_from(rpc_url)?;
            println!("{}", Cast::new(provider).nonce(who, block).await?);
        }
        Subcommands::Wallet { command } => match command {
            WalletSubcommands::New { path, password, unsafe_password } => {
                let mut rng = thread_rng();

                match path {
                    Some(path) => {
                        let password = read_secret(password, unsafe_password)?;
                        let (key, uuid) = LocalWallet::new_keystore(&path, &mut rng, password)?;
                        let address = SimpleCast::checksum_address(&key.address())?;
                        let filepath = format!(
                            "{}/{}",
                            std::fs::canonicalize(path)?
                                .into_os_string()
                                .into_string()
                                .expect("failed to canonicalize file path"),
                            uuid
                        );
                        println!(
                            "Successfully created new keypair at `{}`.\nAddress: {}.",
                            filepath, address
                        );
                    }
                    None => {
                        let wallet = LocalWallet::new(&mut rng);
                        println!(
                            "Successfully created new keypair.\nAddress: {}.\nPrivate Key: {}.",
                            SimpleCast::checksum_address(&wallet.address())?,
                            hex::encode(wallet.signer().to_bytes()),
                        );
                    }
                }
            }
            WalletSubcommands::Vanity { starts_with, ends_with } => {
                let mut regexs = vec![];
                if let Some(prefix) = starts_with {
                    let pad_width = prefix.len() + prefix.len() % 2;
                    hex::decode(format!("{:0>width$}", prefix, width = pad_width))
                        .expect("invalid prefix hex provided");
                    regexs.push(format!(r"^{}", prefix));
                }
                if let Some(suffix) = ends_with {
                    let pad_width = suffix.len() + suffix.len() % 2;
                    hex::decode(format!("{:0>width$}", suffix, width = pad_width))
                        .expect("invalid suffix hex provided");
                    regexs.push(format!(r"{}$", suffix));
                }

                assert!(
                    regexs.iter().map(|p| p.len() - 1).sum::<usize>() <= 40,
                    "vanity patterns length exceeded. cannot be more than 40 characters",
                );

                let regex = RegexSet::new(regexs)?;

                println!("Starting to generate vanity address...");
                let timer = Instant::now();
                let wallet = std::iter::repeat_with(move || LocalWallet::new(&mut thread_rng()))
                    .par_bridge()
                    .find_any(|wallet| {
                        let addr = hex::encode(wallet.address().to_fixed_bytes());
                        regex.matches(&addr).into_iter().count() == regex.patterns().len()
                    })
                    .expect("failed to generate vanity wallet");

                println!(
                    "Successfully created new keypair in {} seconds.\nAddress: {}.\nPrivate Key: {}.",
                    timer.elapsed().as_secs(),
                    SimpleCast::checksum_address(&wallet.address())?,
                    hex::encode(wallet.signer().to_bytes()),
                );
            }
            WalletSubcommands::Address { wallet } => {
                let wallet = EthereumOpts {
                    wallet,
                    from: None,
                    rpc_url: "http://localhost:8545".to_string(),
                }
                .signer(0.into())
                .await?
                .unwrap();

                let addr = match wallet {
                    WalletType::Ledger(signer) => signer.address(),
                    WalletType::Local(signer) => signer.address(),
                    WalletType::Trezor(signer) => signer.address(),
                };
                println!("Address: {}", SimpleCast::checksum_address(&addr)?);
            }
            WalletSubcommands::Sign { message, wallet } => {
                let wallet = EthereumOpts {
                    wallet,
                    from: None,
                    rpc_url: "http://localhost:8545".to_string(),
                }
                .signer(0.into())
                .await?
                .unwrap();

                let sig = match wallet {
                    WalletType::Ledger(wallet) => wallet.signer().sign_message(&message).await?,
                    WalletType::Local(wallet) => wallet.signer().sign_message(&message).await?,
                    WalletType::Trezor(wallet) => wallet.signer().sign_message(&message).await?,
                };
                println!("Signature: 0x{}", sig);
            }
            WalletSubcommands::Verify { message, signature, address } => {
                let pubkey = Address::from_str(&address).expect("invalid pubkey provided");
                let signature = Signature::from_str(&signature)?;
                match signature.verify(message, pubkey) {
                    Ok(_) => {
                        println!("Validation success. Address {} signed this message.", address)
                    }
                    Err(_) => println!(
                        "Validation failed. Address {} did not sign this message.",
                        address
                    ),
                }
            }
        },
    };
    Ok(())
}

fn unwrap_or_stdin<T>(what: Option<T>) -> eyre::Result<T>
where
    T: FromStr + Send + Sync,
    T::Err: Send + Sync + std::error::Error + 'static,
{
    Ok(match what {
        Some(what) => what,
        None => {
            let input = std::io::stdin();
            let mut what = String::new();
            input.read_line(&mut what)?;
            T::from_str(&what.replace('\n', ""))?
        }
    })
}

async fn cast_send<M: Middleware, F: Into<NameOrAddress>, T: Into<NameOrAddress>>(
    provider: M,
    from: F,
    to: T,
    sig: String,
    args: Vec<String>,
    cast_async: bool,
) -> eyre::Result<()>
where
    M::Error: 'static,
{
    let cast = Cast::new(provider);
    let pending_tx =
        cast.send(from, to, if !sig.is_empty() { Some((&sig, args)) } else { None }).await?;
    let tx_hash = *pending_tx;

    if cast_async {
        println!("{}", tx_hash);
    } else {
        let receipt = pending_tx.await?.ok_or_else(|| eyre::eyre!("tx {} not found", tx_hash))?;
        println!("Receipt: {:?}", receipt);
    }

    Ok(())
}
