/*!
# Error types for XML parsing

This module contains the error types used throughout the `xso` crate.
*/

// Copyright (c) 2024 Jonas Schäfer <jonas@zombofant.net>
//
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.
use core::fmt;

use rxml::error::XmlError;

/// Error variants generated while parsing or serialising XML data.
#[derive(Debug)]
pub enum Error {
    /// Invalid XML data encountered
    XmlError(XmlError),

    /// Attempt to parse text data failed with the provided nested error.
    TextParseError(Box<dyn std::error::Error + Send + Sync + 'static>),

    /// Generic, unspecified other error.
    Other(&'static str),

    /// An element header did not match an expected element.
    ///
    /// This is only rarely generated: most of the time, a mismatch of element
    /// types is reported as either an unexpected or a missing child element,
    /// errors which are generally more specific.
    TypeMismatch,
}

impl Error {
    /// Convenience function to create a [`Self::TextParseError`] variant.
    ///
    /// This includes the `Box::new(.)` call, making it directly usable as
    /// argument to [`Result::map_err`].
    pub fn text_parse_error<T: std::error::Error + Send + Sync + 'static>(e: T) -> Self {
        Self::TextParseError(Box::new(e))
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            Self::XmlError(ref e) => write!(f, "xml parse error: {}", e),
            Self::TextParseError(ref e) => write!(f, "text parse error: {}", e),
            Self::TypeMismatch => f.write_str("mismatch between expected and actual XML data"),
            Self::Other(msg) => f.write_str(msg),
        }
    }
}

impl std::error::Error for Error {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::XmlError(ref e) => Some(e),
            Self::TextParseError(ref e) => Some(&**e),
            _ => None,
        }
    }
}

impl From<rxml::error::XmlError> for Error {
    fn from(other: rxml::error::XmlError) -> Error {
        Error::XmlError(other)
    }
}

impl From<rxml::strings::Error> for Error {
    fn from(other: rxml::strings::Error) -> Error {
        Error::XmlError(other.into())
    }
}

impl From<core::convert::Infallible> for Error {
    fn from(other: core::convert::Infallible) -> Self {
        match other {}
    }
}

/// Error returned from
/// [`FromXml::from_events`][`crate::FromXml::from_events`].
#[derive(Debug)]
pub enum FromEventsError {
    /// The `name` and/or `attrs` passed to `FromXml::from_events` did not
    /// match the element's type.
    Mismatch {
        /// The `name` passed to `from_events`.
        name: rxml::QName,

        /// The `attrs` passed to `from_events`.
        attrs: rxml::AttrMap,
    },

    /// The `name` and `attrs` passed to `FromXml::from_events` matched the
    /// element's type, but the data was invalid. Details are in the inner
    /// error.
    Invalid(Error),
}

impl From<Error> for FromEventsError {
    fn from(other: Error) -> Self {
        Self::Invalid(other)
    }
}

impl From<core::convert::Infallible> for FromEventsError {
    fn from(other: core::convert::Infallible) -> Self {
        match other {}
    }
}

impl fmt::Display for FromEventsError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            Self::Mismatch { .. } => f.write_str("element header did not match"),
            Self::Invalid(ref e) => fmt::Display::fmt(e, f),
        }
    }
}

impl std::error::Error for FromEventsError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Mismatch { .. } => None,
            Self::Invalid(ref e) => Some(e),
        }
    }
}

impl From<Error> for Result<minidom::Element, Error> {
    fn from(other: Error) -> Self {
        Self::Err(other)
    }
}

/// Error returned by the `TryFrom<Element>` implementations.
#[derive(Debug)]
pub enum FromElementError {
    /// The XML element header did not match the expectations of the type
    /// implementing `TryFrom`.
    ///
    /// Contains the original `Element` unmodified.
    Mismatch(minidom::Element),

    /// During processing of the element, an (unrecoverable) error occured.
    Invalid(Error),
}

impl fmt::Display for FromElementError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            Self::Mismatch(ref el) => write!(
                f,
                "expected different XML element (got {} in namespace {})",
                el.name(),
                el.ns()
            ),
            Self::Invalid(ref e) => fmt::Display::fmt(e, f),
        }
    }
}

impl std::error::Error for FromElementError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Mismatch(_) => None,
            Self::Invalid(ref e) => Some(e),
        }
    }
}

impl From<Result<minidom::Element, Error>> for FromElementError {
    fn from(other: Result<minidom::Element, Error>) -> Self {
        match other {
            Ok(v) => Self::Mismatch(v),
            Err(e) => Self::Invalid(e),
        }
    }
}

impl From<Error> for FromElementError {
    fn from(other: Error) -> Self {
        Self::Invalid(other)
    }
}

impl From<FromElementError> for Error {
    fn from(other: FromElementError) -> Self {
        match other {
            FromElementError::Invalid(e) => e,
            FromElementError::Mismatch(..) => Self::TypeMismatch,
        }
    }
}

impl From<core::convert::Infallible> for FromElementError {
    fn from(other: core::convert::Infallible) -> Self {
        match other {}
    }
}
