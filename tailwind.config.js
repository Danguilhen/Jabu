/** @type {import('tailwindcss').Config} */
module.exports = {
  content: [
    "./crates/ui/src/**/*.rs",
    "./crates/app/src/**/*.rs",
    "./index.html",
  ],
  theme: {
    extend: {
      fontFamily: {
        sans: ["Inter", "ui-sans-serif", "system-ui", "sans-serif"],
        mono: ["JetBrains Mono", "ui-monospace", "monospace"],
      },
      boxShadow: {
        panel: "0 18px 60px -24px rgba(0, 0, 0, 0.65)",
      },
    },
  },
  plugins: [],
};
