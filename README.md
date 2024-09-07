# gourcers

A Rust program for executing `gource` on many git repositories.

This program can be configured to clone every repo accessible by the GitHub account owning the token provided. You can filter the repos using the `--ignore` and `--ignore-file` options (see [Ignore syntax](#ignore-syntax)).

After cloning the repos, this program will execute `gource` on each of them, saving the generated logs to a text file. Once all repos have had their logs generated, the logs are merged and sorted.

Finally, the merged log is used to generate a video using `gource`. The output is piped into `ffmpeg` and a video will be generated.

## Usage

```
gourcers-ng [OPTIONS] --token <TOKEN>

Options:
  -t, --token <TOKEN>
          Your personal access token for GitHub.

          This token must have the `repo` scope.

          [env: GITHUB_TOKEN=ghp_ZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZ]

  -d, --data-dir <DATA_DIR>
          The directory to store the cloned repos and gource logs.

          If left blank, a temporary directory will be created and removed after finishing.

          If you are going to be running this command multiple times, it is recommended to specify a directory to ensure work is not done multiple times needlessly.

      --temp
          Silently allow using a temporary data directory instead of prompting for confirmation

  -o, --output <OUTPUT>
          The path to output the resulting gource video

          [default: ./gource.mp4]

      --skip-clone
          Skip cloning/pulling repos and assume they are already present in the data directory

  -i, --ignore <IGNORE>
          Ignore any repos matching the given selectors

  -f, --ignore-file <IGNORE_FILE>
          Ignore any repos matching the given selectors from the given file

      --ffmpeg-args <FFMPEG_ARGS>
          Extra arguments to pass to ffmpeg.

          The resulting command will look like `ffmpeg -r 60 -f image2pipe -c:v ppm -i - {ffmpeg_args} {output}`.

          [default: "-c:v libx264 -preset ultrafast -crf 1 -bf 0"]

      --gource-args <GOURCE_ARGS>
          Extra arguments to pass to gource.

          The resulting command will look like `gource {gource_args} -o - {data_dir}/sorted.txt`.

          Using `--hide root` is highly recommended.

          [default: "--hide root -a 1 -s 1 -c 4 --key --multi-sampling -1920x1080"]

  -h, --help
          Print help (see a summary with '-h')

  -V, --version
          Print version
```

## Ignore syntax

# TODO: Rename this from "ignore" to "filter" or something similar

**By default, all repos are ignored**. You must include / exclude repositories based on a few selectors:

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

If you are having trouble tuning your filters, it is recommended that you run the app with `RUST_LOG="gourcers=debug"` to see what repos are being included and excluded, and why. An example output l
