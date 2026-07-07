/** @type {import('tailwindcss').Config} */
module.exports = {
  content: [
    "./pages/**/*.{js,ts,jsx,tsx,mdx}",
    "./components/**/*.{js,ts,jsx,tsx,mdx}",
  ],
  theme: {
    extend: {
      colors: {
        tiger: {
          orange: "#ff6b35",
          dark: "#1a1a2e",
        }
      }
    },
  },
  plugins: [],
}
