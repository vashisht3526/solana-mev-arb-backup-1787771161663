const { Connection, Keypair, PublicKey, Transaction, sendAndConfirmTransaction } = require('./dashboard/node_modules/@solana/web3.js');
const { createCloseAccountInstruction } = require('./dashboard/node_modules/@solana/spl-token');
const bs58 = require('./dashboard/node_modules/bs58');

async function closeAllEmptyATAs() {
  const secretKey = bs58.decode('47CucmhE9pFszCin8rV6TE3ESKNytCwpoe4oTJCcPtYoUPH8KaPwDe6BUuTwKPq5fYdnJXXr2Lx1GoF5ozngVecj');
  const payer = Keypair.fromSecretKey(secretKey);
  const connection = new Connection('https://api.mainnet-beta.solana.com', 'confirmed');

  const tokenAccounts = [
    'A2dW1Y9adLAf3wQivqsgrdRjzaX4Lkg8kpd7X4xDEAsc',
    '5KBzKwYBEY8BJJ9bVzPge3D7kG7HYZH1jz9czrDBkpBW',
    '6Us9Y6S4fPJrzE9DoCSPEoJv99LyMcbLetbQqjCVMhtn',
    '5FZFdTuoQXMXayGa4DMLo9XEok93qRBcEqpJb2aGB4BF'
  ];

  const tx = new Transaction();
  for (const acc of tokenAccounts) {
    tx.add(createCloseAccountInstruction(
      new PublicKey(acc),
      payer.publicKey,
      payer.publicKey
    ));
  }

  const sig = await sendAndConfirmTransaction(connection, tx, [payer]);
  console.log('✅ CLOSED_ATAS_SIGNATURE:', sig);

  const bal = await connection.getBalance(payer.publicKey);
  console.log('💰 RECOVERED_LIQUID_SOL_BALANCE:', (bal / 1e9).toFixed(6), 'SOL');
}

closeAllEmptyATAs().catch(console.error);
