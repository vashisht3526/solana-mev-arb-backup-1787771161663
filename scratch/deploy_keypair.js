const fs = require('fs');
const path = require('path');

const keyBytes = [155,102,247,11,225,143,45,63,112,112,220,7,133,15,73,229,108,203,118,209,158,50,53,7,242,191,77,183,88,141,208,197,136,8,56,249,91,73,1,174,149,33,31,67,125,243,147,129,90,4,16,128,54,156,210,7,187,91,26,103,171,112,220,124];

const targetDir = '/home/ubuntu/.config/solana';
const keyPath = path.join(targetDir, 'mev_live.json');

if (!fs.existsSync(targetDir)) {
    fs.mkdirSync(targetDir, { recursive: true });
}

fs.writeFileSync(keyPath, JSON.stringify(keyBytes));
fs.chmodSync(keyPath, 0o400);

console.log("====================================================");
console.log("✅ REAL TRADING KEYPAIR DEPLOYED ON FRANKFURT SERVER");
console.log("====================================================");
console.log("Key File:", keyPath);
console.log("Public Key: AA1ihbehbPrMW4L9jWtdxJfkH76Up5F2azbeepQ1wixs");
console.log("Permissions: 0400 (Read-Only Owner)");
console.log("====================================================");
