# gourcers

A Rust program for executing `gource` on many git repositories.

This program can be configured to clone every repo accessible by the GitHub account owning the token provided. You can filter the repos using the `--include` and `--include-file` options (see [Include syntax](#include-syntax)).

After cloning the repos, this program will execute `gource` on each of them, saving the generated logs to a text file. Once all repos have had their logs generated, the logs are merged and sorted.

Finally, the merged log is passed into `gource`. This can be [saved to a video](#saving-output-to-a-video).

## Installation

Before installing, make sure you have the following programs installed:

- `git` (also set up SSH keys as `gourcers` only uses SSH URLs)
- `gource`

## Usage

The following command is generally a good starting point:

```sh
# create a GitHub token with the 'repo' scope and store it in GITHUB_TOKEN or use the '-t' argument
export GITHUB_TOKEN="ghp_<...>"
# note: gourcers can also read this from a .env file in the current directory

# replace <your_username> with your GitHub username
gourcers -d ./gourcers-data -i "owner:<your_username>"
```

### Explanation

- `-d ./gourcers-data`: Create a data folder to clone the repos into. Prevents cloning again on the next run.
- `-i "owner:<your_username>"`: Include all repos owned by `<your_username>`. See [Include syntax](#include-syntax) for more selectors.

## Options

```console
$ gourcers --help

A CLI tool for making gource visualizations of multiple repositories

Usage: gourcers [OPTIONS] --token <TOKEN>

Options:
  -t, --token <TOKEN>
          Your personal access token for GitHub.
          
          This token must have the `repo` scope.
          
          [env: GITHUB_TOKEN=]

  -d, --data-dir <DATA_DIR>
          The directory to store the cloned repos and gource logs.
          
          If left blank, a temporary directory will be created and removed after finishing.
          
          If you are going to be running this command multiple times, it is recommended to specify a directory to ensure work is not done multiple times needlessly.

  -y, --temp
          Silently allow using a temporary data directory instead of prompting for confirmation

      --skip-clone
          Skip cloning/pulling repos and assume they are already present in the data directory

  -i, --include <INCLUDE>
          Include any repos matching the given selectors. Can be applied multiple times

  -f, --include-file <INCLUDE_FILE>
          Include any repos matching the given selectors from the given file

      --gource-args <GOURCE_ARGS>
          Extra arguments to pass to gource.
          
          The resulting command will look like `gource {gource_args} {data_dir}/sorted.txt`.
          
          Using `--hide root` is highly recommended.
          
          [default: "--hide root -a 1 -s 1 -c 4 --key --multi-sampling -1920x1080"]

  -h, --help
          Print help (see a summary with '-h')

  -V, --version
          Print version
```

## Include syntax

**By default, all repos are ignored**. You must include / exclude repositories based on a few selectors:

- `*:*`: A special selector that matches all repos
- `name:<value>`: Matches any repos that have the name `<value>`. Ex. `name:gourcers`
- `owner:<value>`: Matches any repos with the owner `<value>`. Ex. `owner:campbellcole`
- `full_name:<value>`: Matches any repos with the full name `<value>`. Ex. `full_name:campbellcole/gourcers`
- `is_fork:<true/false>`: Matches any repos that are (true) or are not (false) forks. Ex. `is_fork:false`

Selectors will include repos matching the given rule. Since all repos are ignored by default, you must add some rules to explicitly include certain repositories, then the inverted rules will filter the repos which have been explicitly included.

To invert a rule, prefix it with `!`, e.g. `!owner:campbellcole`. This will exclude all repositories whose owner is `campbellcole`.

Comments are allowed in an ignore file. Comments are lines that begin with `#`. You may not add a comment to the end of a line. Doing so will cause the `<value>` part of the selector to include the comment, spaces, and `#` character, which is not what you want.

If you are having trouble tuning your filters, you might try running the app with `RUST_LOG="gourcers=debug"` to see which repos are being included and excluded, and why. `gourcers` emits detailed explanations as to why each repository has been included or excluded.

### Examples

#### Include all repositories which are not forks

##### Include File

```yaml
# include all repos whose owner is 'campbellcole'
owner:campbellcole
# exclude all repos which are a fork
!is_fork:true
```

##### Command Arguments

`-i 'owner:campbellcole' -i '!is_fork:true'`

----

#### Include all accessible repositories not owned by your account

##### Include File

```yaml
# include all repos
*:*
# exclude all repos whose owner is 'campbellcole'
!owner:campbellcole
```

##### Command Arguments

`-i '*:*' -i '!owner:campbellcole'`

## Saving output to a video

Previous (unreleased) versions of `gourcers` would pipe the gource output to `ffmpeg`. This behavior has been removed following the [suckless philosophy](https://suckless.org/philosophy/) and [Unix philosophy](https://en.wikipedia.org/wiki/Unix_philosophy).

All new (v1.0.0+) versions support this behavior through piping `gourcers` into `ffmpeg`. `gourcers` writes all progress bars and status updates to stderr, and you can control the `gource` arguments, allowing you to pipe `gource` to stdout.

### Example

```sh
gourcers -d ./data -i 'owner:campbellcole' \
  --gource-args="--hide root -a 1 -s 1 -c 4 --key --multi-sampling -1920x1080 -o -" 2>/dev/null \
  | ffmpeg -r 60 -f image2pipe -c:v ppm -i - -c:v libx264 -preset ultrafast -crf 1 -bf 0 gource.mp4
```

The key parts here are:

- `-o -` in `--gource-args`
- `2>/dev/null` at the end of the `gourcers` invocation
- `-r 60 -f image2pipe -c:v ppm -i -` in the `ffmpeg` args

Everything else can be changed as desired.
