

# Development

Rebuild the NAMESPACE and documentation with:
```r
library(devtools)
document()
```

There's an [example Windows build config](https://yutani.rbind.io/post/some-more-notes-about-using-rust-code-in-r-packages/) that might be good inspiration for building/releasing this for Windows machines.

[gifski](https://cran.r-project.org/web/packages/gifski/index.html) is one of the few packages on CRAN with a Rust build pipeline.

# Installation

Build the R package itself with:
```bash
R CMD INSTALL .
```

# Usage

And then use:
```r
library(entab)
r <- Reader('../test_file.fasta')
data <- as.data.frame(r)
```
