/** @type {import('tailwindcss').Config} */
module.exports = {
    content: ["./src/**/*.rs"],
    darkMode: "class",
    theme: {
        extend: {
            fontFamily: {
                sans: [
                    "Inter", "-apple-system", "BlinkMacSystemFont",
                    "Segoe UI", "Roboto", "sans-serif",
                ],
                mono: [
                    "Berkeley Mono", "JetBrains Mono", "SF Mono",
                    "Fira Code", "monospace",
                ],
            },
            animation: {
                "fade-in": "fadeIn 0.2s ease-out",
                "slide-in": "slideIn 0.25s ease-out",
                "breathe": "breathe 2s ease-in-out infinite",
            },
            keyframes: {
                fadeIn: {
                    "0%": { opacity: "0" },
                    "100%": { opacity: "1" },
                },
                slideIn: {
                    "0%": { opacity: "0", transform: "translateY(4px)" },
                    "100%": { opacity: "1", transform: "translateY(0)" },
                },
                breathe: {
                    "0%, 100%": { opacity: "0.4" },
                    "50%": { opacity: "1" },
                },
            },
        },
    },
    plugins: [],
};
