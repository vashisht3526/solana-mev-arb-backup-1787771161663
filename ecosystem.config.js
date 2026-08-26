module.exports = {
  apps: [
    {
      name: 'rust-mev-arb-dual-live',
      script: './target/release/solana-mev-arb',
      args: '--dual-live',
      env: {
        MEV_MODE: 'dual_live',
        RUST_LOG: 'info'
      },
      cwd: '/home/ubuntu/solana-mev-arb',
      autorestart: true,
      max_memory_restart: '500M'
    },
    {
      name: 'rust-mev-arb-triangular-live',
      script: './target/release/solana-mev-arb',
      args: '--triangular-live',
      env: {
        MEV_MODE: 'triangular_live',
        RUST_LOG: 'info'
      },
      cwd: '/home/ubuntu/solana-mev-arb',
      autorestart: true,
      max_memory_restart: '500M'
    },
    {
      name: 'rust-mev-arb-micro',
      script: './target/release/solana-mev-arb',
      args: '--micro',
      env: {
        MEV_MODE: 'micro',
        RUST_LOG: 'info'
      },
      cwd: '/home/ubuntu/solana-mev-arb',
      autorestart: true,
      max_memory_restart: '500M'
    },
    {
      name: 'mev-dashboard',
      script: 'server.js',
      cwd: '/home/ubuntu/solana-mev-arb/dashboard',
      autorestart: true,
      max_memory_restart: '500M'
    }
  ]
};
