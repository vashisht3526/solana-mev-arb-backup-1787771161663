const { TransactionInstruction, PublicKey, Keypair, Transaction, sendAndConfirmTransaction, Connection } = require('/home/ubuntu/solana-mev-arb/dashboard/node_modules/@solana/web3.js');
const bs58 = require('/home/ubuntu/solana-mev-arb/dashboard/node_modules/bs58');

async function closeAllEmptyATAs() {
  const secretKey = bs58.decode('47CucmhE9pFszCin8rV6TE3ESKNytCwpoe4oTJCcPtYoUPH8KaPwDe6BUuTwKPq5fYdnJXXr2Lx1GoF5ozngVecj');
  const payer = Keypair.fromSecretKey(secretKey);
  const connection = new Connection('https://api.mainnet-beta.solana.com', 'confirmed');

  const preBal = await connection.getBalance(payer.publicKey);
  console.log('💰 Pre-Reclaim Liquid SOL Balance:', (preBal / 1e9).toFixed(6), 'SOL');

  const tokenAccounts = [
    'A2dW1Y9adLAf3wQivqsgrdRjzaX4Lkg8kpd7X4xDEAsc',
    '5KBzKwYBEY8BJJ9bVzPge3D7kG7HYZH1jz9czrDBkpBW',
    '6Us9Y6S4fPJrzE9DoCSPEoJv99LyMcbLetbQqjCVMhtn',
    '5FZFdTuoQXMXayGa4DMLo9XEok93qRBcEqpJb2aGB4BF'
  ];

  const tx = new Transaction();
  for (const acc of tokenAccounts) {
    tx.add(new TransactionInstruction({
      programId: new PublicKey('TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA'),
      keys: [
        { pubkey: new PublicKey(acc), isSigner: false, isWritable: true },
        { pubkey: payer.publicKey, isSigner: false, isWritable: true },
        { pubkey: payer.publicKey, isSigner: true, isWritable: false }
      ],
      data: Buffer.from([9]) // CloseAccount opcode
    }));
  }

  const sig = await sendAndConfirmTransaction(connection, tx, [payer]);
  console.log('✅ [RENT RECLAIMED] Signature: https://solscan.io/tx/' + sig);

  const postBal = await connection.getBalance(payer.publicKey);
  console.log('🏆 [FINAL RECOVERED BALANCE]:', (postBal / 1e9).toFixed(6), 'SOL');
}

closeAllEmptyATAs().catch(console.error);
