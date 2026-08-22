const crypto = require('crypto');
const fs = require('fs');
const path = require('path');

// Base58 alphabet for Solana
const ALPHABET = '123456789ABCDEFGHJKLMNPQRSTUVWXYZabcdefghijkmnopqrstuvwxyz';
function encodeBase58(buffer) {
    const digits = [0];
    for (let i = 0; i < buffer.length; i++) {
        for (let j = 0; j < digits.length; j++) digits[j] <<= 8;
        digits[0] += buffer[i];
        let carry = 0;
        for (let j = 0; j < digits.length; ++j) {
            digits[j] += carry;
            carry = (digits[j] / 58) | 0;
            digits[j] %= 58;
        }
        while (carry) {
            digits.push(carry % 58);
            carry = (carry / 58) | 0;
        }
    }
    for (let i = 0; buffer[i] === 0 && i < buffer.length - 1; i++) digits.push(0);
    return digits.reverse().map(digit => ALPHABET[digit]).join('');
}

const { publicKey, privateKey } = crypto.generateKeyPairSync('ed25519');

const rawPriv = privateKey.export({ type: 'pkcs8', format: 'der' });
const privKeyBytes = rawPriv.slice(rawPriv.length - 32);

const rawPub = publicKey.export({ type: 'spki', format: 'der' });
const pubKeyBytes = rawPub.slice(rawPub.length - 32);

// Standard Solana 64-byte secret key (private + public)
const solanaKeypair = Buffer.concat([privKeyBytes, pubKeyBytes]);
const base58Pub = encodeBase58(pubKeyBytes);

const dir = '/home/ubuntu/.config/solana';
const filePath = path.join(dir, 'mev_live.json');
if (!fs.existsSync(dir)) fs.mkdirSync(dir, { recursive: true });

fs.writeFileSync(filePath, JSON.stringify(Array.from(solanaKeypair)));
fs.chmodSync(filePath, 0o400);

console.log("=================================================");
console.log("🎯 NATIVE LIVE TRADING KEYPAIR GENERATED SAFELY");
console.log("=================================================");
console.log("DEPOSIT_PUBLIC_ADDRESS:", base58Pub);
console.log("KEYPAIR_PATH:", filePath);
console.log("=================================================");
