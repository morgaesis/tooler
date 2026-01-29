const fs = require('fs');
const path = require('path');

const width = 100;
const height = 22;
const prompt = "\x1b[32mme@morgaesis\x1b[0m:\x1b[34m~/tooler\x1b[0m$ ";

const cast = {
  version: 2,
  width: width,
  height: height,
  timestamp: Math.floor(Date.now() / 1000),
  env: { TERM: "xterm-256color" }
};

let currentTime = 0;
const events = [];

function addEvent(text, delay = 0.1) {
  currentTime += delay;
  events.push([currentTime, "o", text]);
}

function typeCommand(cmd) {
  for (const char of cmd) {
    addEvent(char, 0.02 + Math.random() * 0.03);
  }
  addEvent("\r\n", 0.2);
}

function showPrompt(delay = 0.1) {
  addEvent(prompt, delay);
}

// 1. Initial Prompt
showPrompt(0.5);

// 2. GitHub Tool Run
typeCommand("tooler run nektos/act@v0.2.79 --version");
addEvent("Installing/Updating act v0.2.79...\r\n", 0.4);
addEvent("act version 0.2.79\r\n", 0.2);
showPrompt(0.1);

// 3. Pinning
addEvent("", 2.5);
typeCommand("tooler pin nektos/act@v0.2.79");
addEvent("Successfully pinned nektos/act to version 0.2.79\r\n", 0.4);
showPrompt(0.1);

// 4. Complex GitHub Tag
addEvent("", 2.5);
typeCommand("tooler run infisical/infisical@infisical-cli/v0.41.90 --version");
addEvent("Installing/Updating infisical vinfisical-cli/v0.41.90...\r\n", 0.4);
addEvent("0.41.90\r\n", 0.2);
showPrompt(0.1);

// 5. URL Install
addEvent("", 2.5);
typeCommand("tooler run https://dl.k8s.io/release/v1.31.0/bin/linux/arm64/kubectl version --client");
addEvent("Installing/Updating kubectl v1.31.0...\r\n", 0.4);
addEvent("Client Version: v1.31.0\r\n", 0.2);
addEvent("Kustomize Version: v5.4.2\r\n", 0.1);
showPrompt(0.1);

// 6. List
addEvent("", 3.0);
typeCommand("tooler list");
addEvent("--- Installed Tooler Tools ---\r\n", 0.2);
addEvent("  - \x1b[1mdirect/kubectl\x1b[0m (1.31.0) 🔗📌 🚀[binary | arm64 | 0m]\r\n", 0.1);
addEvent("  - \x1b[1minfisical/infisical\x1b[0m (infisical-cli/v0.41.90) 🐙 🚀[binary | arm64 | 0m]\r\n", 0.05);
addEvent("  - \x1b[1mnektos/act\x1b[0m (0.2.79) 🐙📌 📦[archive | arm64 | 0m]\r\n", 0.05);
addEvent("\r\n------------------------------\r\n", 0.1);
showPrompt(0.1);

addEvent("", 4.0);

const output = JSON.stringify(cast) + "\n" + events.map(e => JSON.stringify(e)).join("\n");
fs.writeFileSync('demo.cast', output);
console.log("demo.cast generated.");
