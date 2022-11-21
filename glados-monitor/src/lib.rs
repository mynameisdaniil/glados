use std::time::Duration;

use sea_orm::{ActiveModelTrait, DatabaseConnection, NotSet, Set};

use tokio::sync::mpsc;
use tokio::time::sleep;

use web3::types::BlockId;

use ethereum_types::H256;

use glados_core::types::BlockHeaderContentKey;

use entity::contentid;
use entity::contentkey;

pub mod cli;

pub async fn run_glados_monitor(conn: DatabaseConnection, w3: web3::Web3<web3::transports::Http>) {
    let (tx, rx) = mpsc::channel(100);

    tokio::spawn(follow_chain_head(w3.clone(), tx));
    tokio::spawn(retrieve_new_blocks(w3.clone(), rx, conn));

    tokio::signal::ctrl_c()
        .await
        .expect("failed to pause until ctrl-c");
}

async fn follow_chain_head(
    w3: web3::Web3<web3::transports::Http>,
    tx: mpsc::Sender<web3::types::U64>,
) {
    println!("Initializing block number...");

    let start_block_number = w3
        .eth()
        .block_number()
        .await
        .expect("Failed to fetch initial block number");

    println!("Starting Block Number={}", start_block_number);

    tx.send(start_block_number)
        .await
        .expect("Failed to send new block number");

    // TODO: long running process that fetches latest block...
    let mut block_number = start_block_number;

    loop {
        println!("Sleeping....");
        sleep(Duration::from_secs(5)).await;
        println!("Checking for new block...");

        let candidate_block_number = w3.eth().block_number().await.unwrap();

        if candidate_block_number > block_number {
            block_number = candidate_block_number;
            println!("New block: {}", block_number);
            tx.send(block_number)
                .await
                .expect("Failed to send new block number");
            if block_number > start_block_number + 2 {
                break;
            }
        } else {
            println!("Same block: {}", candidate_block_number);
        }
    }
}

async fn retrieve_new_blocks(
    w3: web3::Web3<web3::transports::Http>,
    mut rx: mpsc::Receiver<web3::types::U64>,
    conn: DatabaseConnection,
) {
    while let Some(block_number_to_retrieve) = rx.recv().await {
        let block = w3
            .eth()
            .block(BlockId::from(block_number_to_retrieve))
            .await
            .expect("Failed to retrieve block");

        // If we got a block back
        if let Some(blk) = block {
            // And if that block has a hash
            if let Some(block_hash) = blk.hash {
                // TODO: convert to log statement
                println!("Received block: hash={}", block_hash);
                let raw_content_key = BlockHeaderContentKey {
                    hash: H256::from_slice(block_hash.as_bytes()),
                };
                let raw_content_id = raw_content_key.content_id();

                // TODO: check if record exists
                let content_id = contentid::ActiveModel {
                    id: NotSet,
                    content_id: Set(raw_content_id.as_bytes().to_vec()),
                };

                let content_id = content_id.insert(&conn).await.unwrap();
                println!("DB content_id.id={}", content_id.id);

                let encoded_content_key = raw_content_key.encoded();

                let content_key = contentkey::ActiveModel {
                    id: NotSet,
                    content_id: Set(content_id.id),
                    content_key: Set(encoded_content_key),
                };

                let content_key = content_key.insert(&conn).await.unwrap();
                println!("DB content_key.id={}", content_key.id);
            }
        } else {
            println!(
                "Failed to retrieve block: number={}",
                block_number_to_retrieve
            );
        }
    }
}