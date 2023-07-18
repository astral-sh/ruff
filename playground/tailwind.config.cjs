/** @type {import('tailwindcss').Config} */
const defaultTheme = require("tailwindcss/defaultTheme");

module.exports = {
  darkMode: "class",
  content: ["./index.html", "./src/**/*.{js,ts,jsx,tsx}"],
  theme: {
    extend: {
      colors: {
        "ayu-accent": "#ffac2f",
        "ayu-background": {
          DEFAULT: "#f8f9fa",
          dark: "#0b0e14",
        },
        galaxy: "#261230",
        space: "#30173D",
        starlight: "#F4F4F1",
        nebula: "#CDCBFB",
        rock: "#78876E",
        radiate: "#D7FF64",
      },
      fontFamily: {
        sans: ["Inter var", ...defaultTheme.fontFamily.sans],
      },
    },
  },
  plugins: [],
};
