import type { MetadataRoute } from "next";

// PWA manifest for the Anseo operator dashboard. Icons are the delivered
// brand reticle assets (public/). Theme color is the locked Signal ink.
export default function manifest(): MetadataRoute.Manifest {
  return {
    name: "Anseo Dashboard",
    short_name: "Anseo",
    description:
      "Local dashboard for Anseo — track your brand's visibility in LLM responses.",
    start_url: "/",
    display: "standalone",
    background_color: "#0A0A0A",
    theme_color: "#0A0A0A",
    icons: [
      {
        src: "/icon-192.png",
        sizes: "192x192",
        type: "image/png",
        purpose: "any",
      },
      {
        src: "/icon-512.png",
        sizes: "512x512",
        type: "image/png",
        purpose: "any",
      },
      {
        src: "/icon-512-maskable.png",
        sizes: "512x512",
        type: "image/png",
        purpose: "maskable",
      },
    ],
  };
}
