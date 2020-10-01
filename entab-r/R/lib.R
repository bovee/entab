#' entab: a package for reading record-oriented file types
#'
#' @importFrom methods new
#' @useDynLib libentab, .registration = TRUE
#'

#' @export Reader
Reader <- setClass("Reader", representation( pointer = "externalptr" ) )

#' Convert the Reader into a data.frame
#' 
#' @export
setMethod("as.data.frame", "Reader", function(x, ...) {
    value <- .Call("wrap__Reader__next", x@pointer)
    df <- as.data.frame(matrix(,0,length(value)))
    names(df) <- .Call("wrap__Reader__headers", x@pointer)
    while (!is.null(value)) {
        # TODO: this is super slow and doesn't scale very well
	df <- rbind(df, value)
        value <- .Call("wrap__Reader__next", x@pointer)
    }
    df
} )

#' Expose methods
#' 
#' i.e. Reader$metadata(), Reader$headers(), and Reader$parser()
setMethod("$", "Reader", function(x, name) {
    function(...) .Call(paste0("wrap__Reader__", name), x@pointer, ...)
} )

#' Pretty-print a description of the Reader
setMethod("show", "Reader", function(object) {
    cat(object$parser(), "Reader\n")
} )

#' Create a new Reader
#'
#' @param .Object base object
#' @param filename path to the file to be parsed
#' @param parser name of the parser to be used; if not specified, auto-detected
#' 
#' @return Reader wrapping the opened file
setMethod("initialize", "Reader", function(.Object, filename, parser = "") {
    .Object@pointer <- .Call("wrap__Reader__new", filename, parser)
    .Object
} )
