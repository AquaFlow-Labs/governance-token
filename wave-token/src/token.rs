use alloy::{
    network::EthereumWallet,
    primitives::{Address, U256},
    providers::{Provider, ProviderBuilder},
    signers::local::PrivateKeySigner,
    sol,
};
use eyre::Result;
use std::str::FromStr;

sol!(
    #[allow(missing_docs)]
    #[sol(rpc)]
    WaveToken,
    "abi/WaveToken.json"
);

pub type Client = impl Provider + Clone;

pub async fn build_client(rpc_url: &str, private_key: &str) -> Result<impl Provider + Clone> {
    let signer: PrivateKeySigner = private_key.parse()?;
    let wallet = EthereumWallet::from(signer);
    let provider = ProviderBuilder::new()
        .with_recommended_fillers()
        .wallet(wallet)
        .on_builtin(rpc_url)
        .await?;
    Ok(provider)
}

fn contract<P: Provider + Clone>(provider: P, address: &str) -> Result<WaveToken::WaveTokenInstance<P>> {
    let addr = Address::from_str(address)?;
    Ok(WaveToken::new(addr, provider))
}

pub async fn name<P: Provider + Clone>(provider: &P, address: &str) -> Result<String> {
    Ok(contract(provider.clone(), address)?.name().call().await?._0)
}

pub async fn symbol<P: Provider + Clone>(provider: &P, address: &str) -> Result<String> {
    Ok(contract(provider.clone(), address)?.symbol().call().await?._0)
}

pub async fn total_supply<P: Provider + Clone>(provider: &P, address: &str) -> Result<U256> {
    Ok(contract(provider.clone(), address)?.totalSupply().call().await?._0)
}

pub async fn balance_of<P: Provider + Clone>(provider: &P, address: &str, account: &str) -> Result<U256> {
    let acc = Address::from_str(account)?;
    Ok(contract(provider.clone(), address)?.balanceOf(acc).call().await?._0)
}

pub async fn mint<P: Provider + Clone>(provider: &P, address: &str, to: &str, amount: U256) -> Result<()> {
    let to_addr = Address::from_str(to)?;
    contract(provider.clone(), address)?.mint(to_addr, amount).send().await?.watch().await?;
    Ok(())
}
