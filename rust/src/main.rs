
#![allow(unused)]
use bitcoin::hex::DisplayHex;
use bitcoincore_rpc::bitcoin::Amount;
use bitcoincore_rpc::{Auth, Client, RpcApi};
use serde::Deserialize;
use serde_json::json;
use std::fs::File;
use std::io::Write;
use bitcoin::Network;

// Node access params
const RPC_URL: &str = "http://127.0.0.1:18443"; // Default regtest RPC port
const RPC_USER: &str = "alice";
const RPC_PASS: &str = "password";

// You can use calls not provided in RPC lib API using the generic `call` function.
// An example of using the `send` RPC call, which doesn't have exposed API.
// You can also use serde_json `Deserialize` derivation to capture the returned json result.
fn send(rpc: &Client, addr: &str) -> bitcoincore_rpc::Result<String> {
    let args = [
        json!([{addr : 100 }]), // recipient address
        json!(null),            // conf target
        json!(null),            // estimate mode
        json!(null),            // fee rate in sats/vb
        json!(null),            // Empty option object
    ];

    #[derive(Deserialize)]
    struct SendResult {
        complete: bool,
        txid: String,
    }
    let send_result = rpc.call::<SendResult>("send", &args)?;
    assert!(send_result.complete);
    Ok(send_result.txid)
}

fn main() -> bitcoincore_rpc::Result<()> {
    // Connect to Bitcoin Core RPC
    let rpc = Client::new(
        RPC_URL,
        Auth::UserPass(RPC_USER.to_owned(), RPC_PASS.to_owned()),
    )?;

    // Get blockchain info
    let blockchain_info = rpc.get_blockchain_info()?;
    println!("Blockchain Info: {:?}", blockchain_info);

    // Create/Load the wallets, named 'Miner' and 'Trader'.
    if !rpc.list_wallets()?.contains(&"Miner".to_string()) {
        rpc.create_wallet("Miner", None, None, None, None)?;
    }
    let miner_rpc = Client::new(
        &format!("{}/wallet/Miner", RPC_URL),
        Auth::UserPass(RPC_USER.to_owned(), RPC_PASS.to_owned()),
    )?;

    if !rpc.list_wallets()?.contains(&"Trader".to_string()) {
        rpc.create_wallet("Trader", None, None, None, None)?;
    }
    let trader_rpc = Client::new(
        &format!("{}/wallet/Trader", RPC_URL),
        Auth::UserPass(RPC_USER.to_owned(), RPC_PASS.to_owned()),
    )?;

    // Generate a new address from the Miner wallet with the label "Mining Reward".
    let miner_address_unchecked = miner_rpc.get_new_address(Some("Mining Reward"), None)?;
    let miner_address = miner_address_unchecked.clone().assume_checked();

    // Mine new blocks to this address until you get a positive wallet balance.
    // A coinbase transaction, which is the reward for mining a block, has a maturity period of 100 blocks.
    // This means the reward from a block can only be spent after 100 additional blocks have been mined.
    // Therefore, we need to mine 101 blocks in total: 1 block for the initial reward and 100 blocks for it to mature.
    miner_rpc.generate_to_address(101, &miner_address)?;
    let miner_balance = miner_rpc.get_balance(None, None)?;
    println!("Miner balance: {}", miner_balance);

    // Create a receiving address from the Trader wallet with the label "Received".
    let trader_address_unchecked = trader_rpc.get_new_address(Some("Received"), None)?;
    let trader_address = trader_address_unchecked.clone().assume_checked();

    // Send a transaction paying 20 BTC from Miner wallet to Trader's wallet.
    let txid = miner_rpc.send_to_address(
        &trader_address,
        Amount::from_btc(20.0)?,
        None,
        None,
        None,
        None,
        None,
        None,
    )?;

    // Fetch the unconfirmed transaction from the node's mempool and print it.
    let mempool_entry = rpc.get_mempool_entry(&txid)?;
    println!("Mempool entry: {:?}", mempool_entry);

    // Confirm the transaction by mining 1 block.
    let block_hash = miner_rpc.generate_to_address(1, &miner_address)?[0];

    // Fetch the details of the confirmed transaction.
    let tx_info = rpc.get_raw_transaction_info(&txid, Some(&block_hash))?;

    let mut output = String::new();
    // Transaction ID
    output.push_str(&format!("{}\n", tx_info.txid));

    // To get the input details, we need to look at the previous transaction that is being spent.
    let vin_txid = &tx_info.vin[0].txid.unwrap();
    let vin_vout = tx_info.vin[0].vout.unwrap();
    let prev_tx = rpc.get_raw_transaction_info(vin_txid, None)?;
    let input_detail = &prev_tx.vout[vin_vout as usize];
    let miner_input_address = input_detail.script_pub_key.address.as_ref().unwrap();
    let miner_input_amount = input_detail.value;
    // Miner's Input Address
    output.push_str(&format!("{}\n", miner_input_address.clone().assume_checked()));
    // Miner's Input Amount
    output.push_str(&format!("{}\n", miner_input_amount.to_btc()));

    // Find the output sent to the Trader.
    let trader_output = tx_info
        .vout
        .iter()
        .find(|o| o.script_pub_key.address.as_ref() == Some(&trader_address_unchecked))
        .unwrap();
    // Trader's Output Address
    output.push_str(&format!("{}\n", trader_output.script_pub_key.address.as_ref().unwrap().clone().assume_checked()));
    // Trader's Output Amount
    output.push_str(&format!("{}\n", trader_output.value.to_btc()));

    // Find the change output sent back to the Miner.
    let miner_change_output = tx_info
        .vout
        .iter()
        .find(|o| o.script_pub_key.address.as_ref() != Some(&trader_address_unchecked))
        .unwrap();
    // Miner's Change Address
    output.push_str(&format!("{}\n", miner_change_output.script_pub_key.address.as_ref().unwrap().clone().assume_checked()));
    // Miner's Change Amount
    output.push_str(&format!("{}\n", miner_change_output.value.to_btc()));

    // Calculate the transaction fees.
    let total_output_amount = tx_info.vout.iter().map(|o| o.value).sum::<Amount>();
    let fees = miner_input_amount - total_output_amount;
    // Transaction Fees
    output.push_str(&format!("{}\n", fees.to_btc()));

    // Get block details for confirmation height and hash.
    let block_info = rpc.get_block_info(&block_hash)?;
    // Block height
    output.push_str(&format!("{}\n", block_info.height));
    // Block hash
    output.push_str(&format!("{}\n", block_info.hash));

    // Write the collected data to ../out.txt.
    let mut file = File::create("../out.txt")?;
    file.write_all(output.as_bytes())?;

    Ok(())
}