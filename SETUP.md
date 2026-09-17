# Setting up Recap Studio

A complete walkthrough, written for people who have never installed a
development tool before. You do not need to understand any of it. Follow the
steps in order and copy the commands exactly.

**Time:** about 30 to 45 minutes the first time, most of it waiting.
**Cost:** the app is free. The AI it talks to is not. See [What this costs](#what-this-costs).

---

## Contents

1. [What you are about to do](#1-what-you-are-about-to-do)
2. [Opening a terminal](#2-opening-a-terminal)
3. [Install Node.js](#3-install-nodejs)
4. [Install Rust](#4-install-rust)
5. [Install the build tools for your computer](#5-install-the-build-tools-for-your-computer)
6. [Download Recap Studio](#6-download-recap-studio)
7. [Install its parts](#7-install-its-parts)
8. [Start the app](#8-start-the-app)
9. [Get an AI key](#9-get-an-ai-key)
10. [Set the app up](#10-set-the-app-up)
11. [Make your first narration](#11-make-your-first-narration)
12. [Optional: video rendering](#12-optional-video-rendering)
13. [Optional: a permanent app icon](#13-optional-a-permanent-app-icon)
14. [If something goes wrong](#if-something-goes-wrong)
15. [What this costs](#what-this-costs)

---

## 1. What you are about to do

Recap Studio is not a normal download-and-run app yet. You are going to build
it on your own computer from its source code. That sounds harder than it is:
you install three things, run two commands, and the app opens.

The three things you install are tools that turn source code into a working
program. You install them once. They stay on your computer and you never think
about them again.

---

## 2. Opening a terminal

The terminal is a window where you type commands instead of clicking. You will
use it several times. Here is how to open one.

**Windows**
Press the **Windows key**, type `powershell`, and press Enter. A blue or black
window opens. That is your terminal.

**Mac**
Press **Command + Space**, type `terminal`, and press Enter. A white or black
window opens.

**Linux**
Press **Ctrl + Alt + T**.

### Two things that will save you

**Copy and paste works.** You never need to type a command by hand. Copy it
from this page and paste it. On Windows, paste with a **right-click**. On Mac,
**Command + V**. On Linux, **Ctrl + Shift + V**.

**Commands finish silently.** When a command is done, you get a fresh line
waiting for the next one. No "Success!" message. That is normal.

---

## 3. Install Node.js

Node.js runs the part of Recap Studio you see on screen.

1. Go to **[nodejs.org](https://nodejs.org)**
2. Click the big green button on the left. It says **LTS** on it. LTS means the
   stable version. Do not pick the other one.
3. Open the file you just downloaded and click Next through the installer.
   Accept every default. Do not change anything.
4. **Close your terminal window completely and open a new one.** This matters.
   A terminal only notices new software when it starts up.

**Check it worked.** Paste this and press Enter:

```
node --version
```

You should see something like `v22.11.0`. Any number starting with 18 or higher
is fine. If you see "command not found", see
[If something goes wrong](#if-something-goes-wrong).

---

## 4. Install Rust

Rust builds the engine underneath the app.

**Windows**

1. Go to **[rustup.rs](https://rustup.rs)**
2. Download and run **rustup-init.exe**
3. A black window opens asking questions. Press **Enter** at every prompt to
   accept the defaults. It may tell you Visual Studio C++ tools are missing and
   offer to install them. Say yes. That is step 5 done for you.
4. Wait for it to finish, then close the terminal and open a new one.

**Mac and Linux**

Paste this into your terminal and press Enter:

```
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
```

When it asks, press **1** and Enter for the default install. When it finishes,
close the terminal and open a new one.

**Check it worked:**

```
cargo --version
```

You should see something like `cargo 1.83.0`. Anything 1.77 or higher is fine.

---

## 5. Install the build tools for your computer

Skip this if the Rust installer already handled it on Windows.

### Windows

You need Microsoft's C++ build tools.

1. Go to **[visualstudio.microsoft.com/visual-cpp-build-tools](https://visualstudio.microsoft.com/visual-cpp-build-tools/)**
2. Download and run the installer
3. A grid of tiles appears. Tick the one called **Desktop development with C++**
4. Click Install, bottom right. This is a big download and may take 15 minutes.
5. Restart your computer when it finishes

**Windows 10 only:** you also need WebView2, which draws the app's window.
Windows 11 already has it. Get it from
**[developer.microsoft.com/microsoft-edge/webview2](https://developer.microsoft.com/en-us/microsoft-edge/webview2/)**
and pick the **Evergreen Standalone Installer**.

### Mac

Paste this into your terminal:

```
xcode-select --install
```

A box pops up. Click **Install** and wait. If it says the tools are already
installed, you are done.

### Linux (Ubuntu, Debian, Mint, Pop!_OS)

Paste these two commands, one at a time:

```
sudo apt update
```

```
sudo apt install libwebkit2gtk-4.1-dev build-essential curl wget file libxdo-dev libssl-dev libayatana-appindicator3-dev librsvg2-dev
```

It will ask for your password. Type it and press Enter. Nothing appears as you
type, not even dots. That is normal and your password is going in.

On Fedora, openSUSE or Arch, see
[Tauri's prerequisites page](https://v2.tauri.app/start/prerequisites/) for the
equivalent package names.

---

## 6. Download Recap Studio

1. Go to **[github.com/ajmarkley1-web/recap-studio](https://github.com/ajmarkley1-web/recap-studio)**
2. Click the green **Code** button
3. Click **Download ZIP**
4. Find the file in your Downloads folder and unzip it. On Windows, right-click
   it and choose **Extract All**. On Mac, double-click it.

You now have a folder called `recap-studio-main`. Move it somewhere you will
remember, like your Desktop.

### Getting your terminal into that folder

Your terminal needs to be "inside" the folder to run the app. There is a trick
for this so you do not have to type a long path.

1. In your terminal, type `cd ` — that is c, d, then **a space**. Do not press
   Enter yet.
2. Drag the `recap-studio-main` folder from your Desktop into the terminal
   window and let go. The path appears by itself.
3. Now press Enter.

The text to the left of your cursor changes to show the folder name. You are in.

---

## 7. Install its parts

Paste this and press Enter:

```
npm install
```

This downloads the pieces the app is built from. It takes one to three minutes
and prints a lot of text. You may see yellow warnings about "deprecated"
packages. **Ignore them.** They are normal and harmless.

When you get a fresh line back, it is done.

---

## 8. Start the app

Paste this and press Enter:

```
npm run app
```

### The first time takes a long time. This is the part people give up on.

The first run compiles the entire engine from scratch. **Expect 5 to 20 minutes**
depending on your computer. You will see hundreds of lines scroll past saying
`Compiling` followed by names you do not recognise.

**None of that is an error.** The app is being built. Leave the window alone and
let it work. Some lines say `warning:` — those are also fine and can be ignored.

When it finishes, the Recap Studio window opens by itself.

Every time after this, it starts in a few seconds.

**To stop the app:** close its window, then click the terminal and press
**Ctrl + C**.

---

## 9. Get an AI key

Recap Studio does not include an AI. It borrows one, and you tell it which.
A key is a long password that lets the app use your account.

Pick **one** of these.

| Provider | Where to get a key | Notes |
| --- | --- | --- |
| **OpenAI** | [platform.openai.com/api-keys](https://platform.openai.com/api-keys) | Easiest to start. Needs a card and about $5 of credit. |
| **Anthropic** | [console.anthropic.com](https://console.anthropic.com) | Strong at long writing. Needs a card. |
| **Google Gemini** | [aistudio.google.com/apikey](https://aistudio.google.com/apikey) | Has a free tier. |
| **Ollama** | [ollama.com](https://ollama.com) | Free, runs on your own computer, no card. Slower, and needs a powerful machine. |

Click **Create new secret key**, then **copy it immediately**. Most sites show
the key once and never again. Paste it somewhere safe for a moment.

> **Treat the key like a credit card number.** Anyone who has it can spend your
> money. Do not post it, screenshot it, or paste it into a chat. Recap Studio
> stores it on your own computer only, and never inside a project folder, so
> projects are safe to share.

---

## 10. Set the app up

In the Recap Studio window:

1. Click **Settings** in the sidebar
2. Find your provider in the **Providers** list and click it to expand
3. Paste your key into the box and click **Save**
4. Scroll to **Models** and click **Refresh** next to the dropdown. The list
   loads from your provider, so new models show up the day they launch.
5. Pick a model. **It must have `[vision]` next to it.** The app reads pages as
   pictures, so a model that cannot see images will not work.

That is the whole setup. You are ready.

---

## 11. Make your first narration

1. Click **Projects**, then **New project**
2. Give it a name and choose the format:
   - **Manhwa** for Korean webtoons, the tall scrolling kind
   - **Manga** for Japanese pages that read right to left
   - **Comic** for Western pages that read left to right
3. Drag in your chapter. A PDF, a folder of images, or loose image files all work.
4. Click **Analyze** in the sidebar, then the **Analyze** button. The app reads
   every page and works out who is in it and what happens. This is the slow,
   expensive step.
5. Click **Script** in the sidebar. This is the narration screen.
6. Click **Narrate**. You get a full narration, written panel by panel in order.
7. It appears under the **Narration** tab. Check the panel count above it — if
   it says `48/48 panels` in green, nothing was skipped. Orange means some
   panels were missed, and a **Fix** button appears to write them in.

### Changing how it writes

On the **Script** screen, open the **Engine** tab. The narrator prompt and the
delivery rules are both there as editable text, and every automatic check has an
on/off switch. Change
whatever you like and press **Save engine**. If an experiment goes wrong, the
**Reset to default** button puts the original back.

One warning the app will give you: if you edit the delivery rules and remove the
`[[page:panel]]` tags, panel tracking stops working and the storyboard comes back
empty. The app tells you before you run it, but do not delete those tags unless
you mean to.

---

## 12. Optional: video rendering

Everything up to and including the narration works without this. You only need
it to turn a narration into a finished video.

**Windows** — paste into your terminal:

```
winget install Gyan.FFmpeg
```

**Mac** — needs [Homebrew](https://brew.sh) first, then:

```
brew install ffmpeg
```

**Linux:**

```
sudo apt install ffmpeg
```

Close Recap Studio and open it again afterwards. It will find ffmpeg by itself.

---

## 13. Optional: a permanent app icon

So far you start the app by typing a command. To get a real installed app with
an icon instead:

```
npm run app:build
```

This takes another 5 to 15 minutes. When it finishes, look inside the project
folder at `src-tauri/target/release/bundle/`. You will find an installer there:
a `.msi` on Windows, a `.dmg` on Mac, a `.deb` or `.AppImage` on Linux. Run it
like any normal installer.

---

## If something goes wrong

| What you see | What it means | What to do |
| --- | --- | --- |
| `npm: command not found` | Node.js is not installed, or the terminal has not noticed it | Close the terminal, open a new one, try again. Still failing? Reinstall from [step 3](#3-install-nodejs). |
| `cargo: command not found` | Same thing for Rust | Close the terminal, open a new one. Still failing? Reinstall from [step 4](#4-install-rust). |
| `linker 'link.exe' not found` (Windows) | The C++ build tools are missing | Do [step 5](#5-install-the-build-tools-for-your-computer) and restart your computer. |
| `error: failed to run custom build command for gdk-sys` or `webkit2gtk not found` (Linux) | System libraries are missing | Run the `apt install` line in [step 5](#5-install-the-build-tools-for-your-computer). |
| Hundreds of `Compiling` lines, seems stuck | Nothing is wrong | This is the normal first build. Wait. It can take 20 minutes. |
| Yellow `warning:` lines | Nothing is wrong | Ignore them. Only red `error:` lines matter. |
| `No model selected yet` in the app | No model has been chosen | Settings → Models → Refresh → pick one tagged `[vision]`. |
| The model list is empty after Refresh | The key is wrong, or has no credit | Re-copy the key. Check your provider account has credit. |
| `401` or `invalid api key` | The key is wrong or was revoked | Make a new key and paste it again. Watch for stray spaces. |
| ffmpeg not found | Only affects video | [Step 12](#12-optional-video-rendering), or ignore it and use the written narration. |
| `no space left on device` | Your disk is full | The build needs several GB. Free some space and retry. |
| Panel count says e.g. `31/48 panels` | The AI skipped some panels | Click **Fix** in the Rule compliance box. It rewrites only the missing ones. |
| Sidebar says **Script** but the screen says **Narration** | Two names for one screen | They are the same place. Use the **Script** item in the sidebar. |

Still stuck? [Open an issue](https://github.com/ajmarkley1-web/recap-studio/issues/new)
and paste in the **red** error lines, plus which system you are on.

---

## What this costs

The app is free. The AI charges per page it reads, and reading pages is most of
the bill.

Three settings control it, under **Settings → Analyze engine**:

- **Pages per AI call** — more pages per call is cheaper. Four is a sensible middle.
- **Image size** — AI providers charge by how big the picture is. 1280 reads most
  speech bubbles. Lower it to save money, raise it for dense artwork.
- **Send individual panels instead of whole pages** — much more accurate and much
  more expensive. Off by default. Leave it off until you need it.

You can also split the work: a cheap fast model for reading pages, a stronger one
for the writing. That is under **Settings → Models → Use different models per job**,
and it is where most of the savings are.

The Analyze screen shows a running token total after each run, so you can watch
what a chapter actually costs before committing to a whole series.

Or use **Ollama**, which runs on your own computer and costs nothing at all.
