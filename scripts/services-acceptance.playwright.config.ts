import { defineConfig } from "@playwright/test";

export default defineConfig({
  testDir: ".",
  testMatch: "services-acceptance.spec.ts",
  fullyParallel: false,
  timeout: 300_000,
  use: {
    baseURL:
      process.env.FILEBEAM_ACCEPTANCE_INSTANCE ?? "http://127.0.0.1:8019",
  },
  reporter: "list",
});
