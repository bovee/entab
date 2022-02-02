#' entab: a package for reading record-oriented file types
#'
#' @importFrom methods new
#' @useDynLib libentab, .registration = TRUE
#'

#' @export Reader
Reader <- setClass("Reader", slots = c( pointer = "externalptr" ) )

#' Convert the Reader into a data.frame
#' 
#' @export
setMethod("as.data.frame", "Reader", function(x, ...) {
    .Call("wrap__as_data_frame", x@pointer)
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
    d <- .Call("wrap__Reader__new", filename, parser)
    # extendr is setting class, but we need to strip it to fit in the slot
    attr(d, "class") <- NULL
    .Object@pointer <- d
    .Object
} )
