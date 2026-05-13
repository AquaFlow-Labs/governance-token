mod token;

use eyre::Result;

#[tokio::main]
async fn main() -> Result<()> {
    dotenvy::dotenv().ok();

    let rpc_url = std::env::var("RPC_URL")?;
    let contract_addr = std::env::var("CONTRACT_ADDRESS")?;
    let private_key = std::env::var("PRIVATE_KEY")?;

    let client = token::build_client(&rpc_url, &private_key).await?;

    println!("name:         {}", token::name(&client, &contract_addr).await?);
    println!("symbol:       {}", token::symbol(&client, &contract_addr).await?);
    println!("total supply: {}", token::total_supply(&client, &contract_addr).await?);

    Ok(())
}
