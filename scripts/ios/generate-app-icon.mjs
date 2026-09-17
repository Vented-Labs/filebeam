#!/usr/bin/env node
import { readFile, writeFile } from "node:fs/promises";
import { Resvg } from "@resvg/resvg-js";

const [source, output] = process.argv.slice(2);
if (!source || !output) throw new Error("usage: generate-app-icon.mjs SOURCE.svg OUTPUT.png");

const svg = await readFile(source);
const png = new Resvg(svg, { fitTo: { mode: "width", value: 1024 } }).render().asPng();
await writeFile(output, png);
