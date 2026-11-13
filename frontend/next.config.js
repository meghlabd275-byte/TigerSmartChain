/** @type {import('next').NextConfig} */
const nextConfig = {
  // Emit a self-contained server bundle so the Docker runtime image can copy
  // `.next/standalone` and run `node server.js` without node_modules.
  output: 'standalone',
  reactStrictMode: true,
  images: {
    domains: ['raw.githubusercontent.com', 'ipfs.io', 'cloudflare-ipfs.com'],
  },
  env: {
    NEXT_PUBLIC_API_URL: process.env.NEXT_PUBLIC_API_URL || 'http://localhost:8080',
    NEXT_PUBLIC_WS_URL: process.env.NEXT_PUBLIC_WS_URL || 'ws://localhost:8081',
  },
}

module.exports = nextConfig
