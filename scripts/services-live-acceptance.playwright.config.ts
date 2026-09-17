import { defineConfig } from "@playwright/test";

export default defineConfig({
  testDir: ".",
  testMatch: "services-live-acceptance.spec.ts",
  fullyParallel: false,
  timeout: 180_000,
  use: {
    baseURL:
      process.env.FILEBEAM_ACCEPTANCE_INSTANCE ?? "http://127.0.0.1:8027",
  },
  reporter: "list",
});
