# gourcers

A tool for running `gource` on many repos at the same time. Given a GitHub token and a special ignore file (see [Ignore syntax](#ignore-syntax)), this tool will clone and gource each repo individually, in parallel, and then merge the gource logs together and then execute gource on the log.

## Compatibility

This crate has only been tested on Linux. It should work for macOS and Windows with the appropriate dependencies installed. If you run into problems on any OS, please [open an issue](https://github.com/campbellcole/gourcers/issues).

## Runtime dependencies

In order to use this crate, you must have the following packages installed and on your `PATH`:

- `git`
  - Make sure you have SSH keys set up locally and on GitHub. This crate currently only uses SSH URLs.
- `gource`
  - For Windows, download the setup binary from [gource.io](https://gource.io)
  - For macOS, install the latest version using homebrew: `brew install gource`
  - For Linux, install the latest version using your package manager: [repology](https://repology.org/project/gource/versions)
- `ffmpeg`
  - For Windows, download the binaries from the [ffmpeg downloads page](https://ffmpeg.org/download.html)
  - For macOS, intall the latest version using homebrew: `brew install ffmpeg`
  - For Linux, install the latest version using your package manager: [repology](https://repology.org/project/ffmpeg/versions)
- `qsv`
  - For all platforms, install using `cargo install qsv --features full`

## Basic usage

On GitHub, create a new [personal access token](<https://github.com/settings/tokens/new?description=gourcers%20(repo)&scopes=repo>) with the `repo` scope. This is necessary for grabbing a list your repositories through the GitHub API so they can be cloned.
<br/><small><small>To see how this token is used, check the [`src/gh.rs`](/src/gh.rs) file.</small></small>

Once you have this token, either put it in a `.env` file as shown in [`.env.example`](/.env.example), or pass it as a command line argument with `-t` or `--token` (not recommended).

Next, designate a folder to store all of the cloned repos and gource logs. This crate will create the folder if it does not exist, so just choose where you'd like this data folder.

Finally, execute this crate using `gourcers --data-folder <data folder>`.

## Detailed usage

This crate has quite a few options. You can see them all using `gourcers --help`:

- `-t <token>` or `--token <token>`: Pass the GitHub token as an argument rather than reading it from an environment variable.
- `-o <data_folder>` or `--data-folder <data_folder>`: The directory to store the cloned repos and gource logs.
  - It is recommended that you keep this directory if you are going to run this multiple times. If the repos already exist, this crate will `git pull` them instead of cloning again.
- `-d` or `--dump`: Dump the list of repos to a JSON file in the CWD. This prevents unneccesary calls to the GitHub API.
  - If you are going to run this crate multiple times consecutively, it is recommended that you enable this flag for the first run, then use the `--use-dump` argument for all consecutive calls.
- `-u` or `--use-dump`: Use the dump created by the `--dump` option rather than calling the GitHub API again.
- `--dump-requests`: Dump the raw data from the GitHub API to a sequence of JSON files in the CWD.
  - Only used for debugging.
- `-i <rules...>` or `--ignore <rules...>`: Add a single ignore rule. Can be called multiple times. (See [Ignore syntax](#ignore-syntax))
  - This option is ignored if the `--ignores-file` argument is present.
- `-f <ignores_file>` or `--ignores-file <ignores_file>`: A file containing a set of ignore rules, separated by newlines. (See [Ignore syntax](#ignore-syntax))
- `-s <n>` or `--stop-after <n>`: Stop gourcing/cloning after `n` repos. Mostly for debugging.
- `--dry-run`: Fetch and filter repositories, but do not clone or gource them.
  - Useful for debugging ignore rules in combination with `RUST_LOG='gourcers=trace'`.
- `-g` or `--video`: Use `ffmpeg` to generate a video from the gource PPM stream.
  - The resulting video has a CRF of 1, resulting in a massive file size. It is recommended that you compress this file manually.
- `-v <filename>` or `--video-filename <filename>`: Set the video filename.
  - Defaults to `gource.mp4`.
- `-r <WIDTHxHEIGHT[!]>` or `--video-resolution <WIDTHxHEIGHT[!]>`: Set the resolution argument for `gource`.
  - This is not validated, so only use resolutions supported by gource's `--viewport` argument. Takes the format `WIDTHxHEIGHT`.
  - Adding a `!` to the end will disable resizing the window.
  - Defaults to `1920x1080`.
- `-p <options>` or `--gource-options <options>`: Set the options given to gource.
  - This crate may have a tendency to detect the value of this argument as a list of arguments itself. To get around this, use `--gource-options='<args>'`.
  - Using `--hide root` is recommended for best results.
  - Defaults to `--hide root -a 1 -s 1 -c 4 --key --multi-sampling`.
- `-h` or `--help`: Print the help message.
- `-V` or `--version`: Print the crate version.

## Ignore syntax

By default, all repos are ignored, and you must include / exclude repositories based on a few selectors:

- `*:*`: A special selector that matches all repos
- `name:<value>`: Matches any repos that have the name `<value>`. Ex. `name:gourcers`
- `owner:<value>`: Matches any repos with the owner `<value>`. Ex. `owner:campbellcole`
- `full_name:<value>`: Matches any repos with the full name `<value>`. Ex. `full_name:campbellcole/gourcers`
- `is_fork:<true/false>`: Matches any repos that are (true) or are not (false) forks. Ex. `is_fork:false`

Selectors will exclude repos matching the given rule. Since all repos are ignored by default, you must invert some rules to explicitly allow certain repositories, then the exclude rules will filter the repos which have been explicitly included.

To invert a rule, prefix it with `!`, ex. `!owner:campbellcole`. This will include all repositories whose owner is `campbellcole`.

To include all of your repositories which are not forks, you would use something like:

```
# include all repos whose owner is 'campbellcole'
!owner:campbellcole
# exclude all repos which are a fork
is_fork:true
```

To include all repos accessible by your account which are not your repos, you would use something like:

```
# include all repos
!*:*
# exclude all repos whose owner is 'campbellcole'
owner:campbellcole
```

Comments are allowed in an ignore file. Comments are lines that begin with `#`. You may not add a comment to the end of a line. Doing so will cause the `<value>` part of the selector to include the comment, spaces, and `#` character, which is not what you want.
