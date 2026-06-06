import type { Metadata } from "next";
import { JetBrains_Mono } from "next/font/google";

import { ShellGate } from "@/app/_components/shell-gate";

import "./globals.css";

// Read `anseo-theme` from localStorage; fall back to `dark` (Signal default).
// Separate try/catches so a localStorage throw (Safari Private mode, quota)
// doesn't short-circuit the dark fallback.
const THEME_INIT_SCRIPT = `(function(){var s;try{s=localStorage.getItem('anseo-theme');}catch(_){}var t=(s==='light'||s==='dark')?s:'dark';document.documentElement.setAttribute('data-theme',t);})();`;

const jetbrainsMono = JetBrains_Mono({
  variable: "--font-jetbrains-mono",
  subsets: ["latin"],
  display: "swap",
});

export const metadata: Metadata = {
  metadataBase: new URL("http://localhost:3000"),
  title: "Anseo Dashboard",
  description:
    "Local dashboard for Anseo — track your brand's visibility in LLM responses.",
  // Favicon / app icons (app/icon.svg, app/icon.png, app/apple-icon.png),
  // the PWA manifest (app/manifest.ts), and social images
  // (app/opengraph-image.png, app/twitter-image.png) are auto-detected by the
  // Next.js app-router metadata file conventions.
};

export default function RootLayout({
  children,
}: Readonly<{
  children: React.ReactNode;
}>) {
  return (
    <html
      lang="en"
      className={`${jetbrainsMono.variable} h-full`}
      suppressHydrationWarning
    >
      <head>
        <script dangerouslySetInnerHTML={{ __html: THEME_INIT_SCRIPT }} />
      </head>
      <body className="min-h-full">
        <ShellGate>{children}</ShellGate>
      </body>
    </html>
  );
}
