

# Development

Rebuild the NAMESPACE and documentation with:
```r
library(devtools)
document()
```

Build the R package itself with:
```bash
R CMD INSTALL .
```

And then use:
```r
library(entab)
r <- Reader('../test_file.fasta')
data <- as.data.frame(r)
```
