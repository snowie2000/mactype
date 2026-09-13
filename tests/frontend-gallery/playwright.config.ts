import { defineConfig, devices } from "@playwright/test";
import path from "node:path";

const repositoryRoot = path.resolve(__dirname, "../..");
const galleryRoot = path.join(repositoryRoot, "artifacts", "frontend-gallery");

export default defineConfig({
  testDir: ".",
  testMatch: "gallery.spec.ts",
  fullyParallel: false,
  forbidOnly: Boolean(process.env.CI),
  retries: process.env.CI ? 1 : 0,
  reporter: [["list"], ["html", { outputFolder: path.join(galleryRoot, "report"), open: "never" }]],
  outputDir: path.join(galleryRoot, "results"),
  use: {
    baseURL: "http://127.0.0.1:4173",
    trace: "retain-on-failure",
  },
  projects: [
    { name: "mobile-390", use: { ...devices["Desktop Chrome"], viewport: { width: 390, height: 844 } } },
    { name: "tablet-768", use: { ...devices["Desktop Chrome"], viewport: { width: 768, height: 900 } } },
    { name: "desktop-1280", use: { ...devices["Desktop Chrome"], viewport: { width: 1280, height: 800 } } },
  ],
  webServer: {
    command: "pnpm preview --host 127.0.0.1",
    cwd: path.join(repositoryRoot, "control-center"),
    port: 4173,
    reuseExistingServer: !process.env.CI,
  },
});
