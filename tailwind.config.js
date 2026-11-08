/** @type {import('tailwindcss').Config} */
const shades = [50, 100, 200, 300, 400, 500, 600, 700, 800, 900, 950];

const themeScale = (name) =>
  Object.fromEntries(
    shades.map((shade) => [shade, `rgb(var(--${name}-${shade}) / <alpha-value>)`]),
  );

module.exports = {
  content: [
    "./crates/ui/src/**/*.rs",
    "./crates/app/src/**/*.rs",
    "./index.html",
  ],
  theme: {
    extend: {
      colors: {
        gray: themeScale("gray"),
        teal: themeScale("teal"),
        red: themeScale("red"),
        green: themeScale("green"),
        emerald: themeScale("green"),
        amber: themeScale("amber"),
        yellow: themeScale("amber"),
        sky: themeScale("sky"),
      },
      fontFamily: {
        sans: [
          "IBM Plex Sans",
          "Noto Sans",
          "Noto Sans Arabic",
          "Noto Sans CJK SC",
          "Noto Sans Devanagari",
          "ui-sans-serif",
          "system-ui",
          "sans-serif",
        ],
        mono: ["IBM Plex Mono", "ui-monospace", "monospace"],
      },
      boxShadow: {
        panel: "var(--shadow-panel)",
      },
    },
  },
  plugins: [],
};
