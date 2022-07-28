# Development

Rebuild the NAMESPACE and documentation with:
```r
library(devtools)
document()
```

Note that there's an issue with having the entab dependency in the R bindings as a path (and including this in the workspace in the directory above) because R will only build this directory and not include the parent directory. This will cause the build process to fail with a message about "could not find entab, only entab-r". What this means in practice is that a new version of `entab` needs to be pinned in Crates before any new features can be used in here.

For future inspiration: There's an [example Windows build config](https://yutani.rbind.io/post/some-more-notes-about-using-rust-code-in-r-packages/) that might be good inspiration for building/releasing this for Windows machines.  [gifski](https://cran.r-project.org/web/packages/gifski/index.html) is one of the few packages on CRAN with a Rust build pipeline.

# Installation

Build the R package itself with:
```bash
R CMD INSTALL .
```

You can also install off of Github with:
```r
library(devtools)
devtools::install_github("bovee/entab", subdir="entab-r")
```

## Additional instructions for Mac OS X installation

If you're using RStudio on a Mac, you will likely need to tell R Studio where to find cargo (the Rust package manager) by adding it to your path. You can do this by modifying your `.Rprofile` file as suggested below:

1. Find your `.Rprofile` file in your home directory (you may need to press Command + Shift + period to reveal hidden files) and open it in your text editor of choice. Alternatively, you can call `usethis::edit_r_profile()` from RStudio to automatically open your `.Rprofile`. 
2. Add `Sys.setenv(PATH = paste0("/Users/<user>/.cargo/bin:", Sys.getenv("PATH")))`, replacing <user> with your username. This will append Cargo to your path when you open RStudio.
3. Save your `.Rprofile` and restart R Studio.
4. Install Entab from GitHub.

# Usage

And then use:
```r
library(entab)
r <- Reader('../test_file.fasta')
data <- as.data.frame(r)
```
