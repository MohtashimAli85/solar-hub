import { isMainModule, sunInfo } from "./lib.js";

async function readStdin() {
  const chunks = [];
  for await (const chunk of process.stdin) chunks.push(chunk);
  return Buffer.concat(chunks).toString("utf8");
}

export function run({ latitude, longitude }, date = new Date()) {
  return sunInfo(latitude, longitude, date);
}

if (isMainModule(import.meta.url)) {
  readStdin()
    .then((raw) => run(JSON.parse(raw)))
    .then((result) => process.stdout.write(JSON.stringify(result)))
    .catch((error) => {
      console.error(error);
      process.exit(1);
    });
}
