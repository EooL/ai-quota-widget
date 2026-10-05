/** @type {import('tailwindcss').Config} */
export default {
  content: ["./index.html", "./src/**/*.{js,ts,jsx,tsx}"],
  theme: {
    extend: {
      fontFamily: {
        sans: ['"IBM Plex Sans"', "system-ui", "sans-serif"],
        mono: ['"Space Mono"', "ui-monospace", "monospace"],
      },
      colors: {
        chatgpt: "#10a37f",
        gemini: "#4285f4",
        claude: "#d97706",
      },
    },
  },
  plugins: [],
};
