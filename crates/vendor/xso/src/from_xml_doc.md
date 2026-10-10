# Make a struct or enum parseable from XML

This derives the [`FromXml`] trait on a struct or enum. It is the counterpart
to [`macro@AsXml`].

## Example

```rust
# use xso::FromXml;
#[derive(FromXml, Debug, PartialEq)]
#[xml(namespace = "urn:example", name = "foo")]
struct Foo;

let foo: Foo = xso::from_bytes(b"<foo xmlns='urn:example'/>").unwrap();
assert_eq!(foo, Foo);
```

## Attributes

The derive macros need additional information, such as XML namespaces and
names to match. This must be specified via key-value pairs on the type or
fields the derive macro is invoked on. These key-value pairs are specified as
Rust attributes. In order to disambiguate between XML attributes and Rust
attributes, we are going to refer to Rust attributes using the term *meta*
instead, which is consistent with the Rust language reference calling that
syntax construct *meta*.

All key-value pairs interpreted by these derive macros must be wrapped in a
`#[xml( ... )]` *meta*.

The values associated with the keys may be of different types, defined as
such:

- *path*: A Rust path, like `some_crate::foo::Bar`. Note that `foo` on its own
  is also a path.
- *string literal*: A string literal, like `"hello world!"`.
- *type*: A Rust type.
- flag: Has no value. The key's mere presence has relevance and it must not be
  followed by a `=` sign.

### Struct meta

The following keys are defined on structs:

| Key | Value type | Description |
| --- | --- | --- |
| `namespace` | *string literal* or *path* | The XML element namespace to match. If it is a *path*, it must point at a `&'static str`. |
| `name` | *string literal* or *path* | The XML element name to match. If it is a *path*, it must point at a `&'static NcNameStr`. |

Note that the `name` value must be a valid XML element name, without colons.
The namespace prefix, if any, is assigned automatically at serialisation time
and cannot be overridden. The following will thus not compile:

```compile_fail
# use xso::FromXml;
#[derive(FromXml, Debug, PartialEq)]
#[xml(namespace = "urn:example", name = "fnord:foo")]  // colon not allowed
struct Foo;
```

### Field meta

For fields, the *meta* consists of a nested meta inside the `#[xml(..)]` meta,
the identifier of which controls *how* the field is mapped to XML, while the
contents control the parameters of that mapping.

The following mapping types are defined:

| Type | Description |
| --- | --- |
| [`attribute`](#attribute-meta) | Map the field to an XML attribute on the struct's element |
| [`child`](#child-meta) | Map the field to a child element |
| [`text`](#text-meta) | Map the field to the text content of the struct's element |

#### `attribute` meta

The `attribute` meta causes the field to be mapped to an XML attribute of the
same name. For `FromXml`, the field's type must implement [`FromXmlText`] and
for `AsXml`, the field's type must implement [`AsOptionalXmlText`].

The following keys can be used inside the `#[xml(attribute(..))]` meta:

| Key | Value type | Description |
| --- | --- | --- |
| `namespace` | *string literal* or *path* | The optional namespace of the XML attribute to match. If it is a *path*, it must point at a `&'static str`. Note that attributes, unlike elements, are unnamespaced by default. |
| `name` | *string literal* or *path* | The name of the XML attribute to match. If it is a *path*, it must point at a `&'static NcNameStr`. |
| `default` | flag | If present, an absent attribute will substitute the default value instead of raising an error. |

If the `name` key contains a namespace prefix, it must be one of the prefixes
defined as built-in in the XML specifications. That prefix will then be
expanded to the corresponding namespace URI and the value for the `namespace`
key is implied. Mixing a prefixed name with an explicit `namespace` key is
not allowed.

The `attribute` meta also supports a shorthand syntax,
`#[xml(attribute = ..)]`, where the value is treated as the value for the
`name` key (with optional prefix as described above, and unnamespaced
otherwise).

If `default` is specified and the attribute is absent in the source, the value
is generated using [`std::default::Default`], requiring the field type to
implement the `Default` trait for a `FromXml` derivation. `default` has no
influence on `AsXml`.

##### Example

```rust
# use xso::FromXml;
#[derive(FromXml, Debug, PartialEq)]
#[xml(namespace = "urn:example", name = "foo")]
struct Foo {
    #[xml(attribute)]
    a: String,
    #[xml(attribute = "bar")]
    b: String,
    #[xml(attribute(name = "baz"))]
    c: String,
    #[xml(attribute(namespace = "urn:example", name = "fnord"))]
    d: String,
    #[xml(attribute = "xml:lang")]
    e: String,
};

let foo: Foo = xso::from_bytes(b"<foo
    xmlns='urn:example'
    a='1' bar='2' baz='3'
    xmlns:tns0='urn:example' tns0:fnord='4'
    xml:lang='5'
/>").unwrap();
assert_eq!(foo, Foo {
    a: "1".to_string(),
    b: "2".to_string(),
    c: "3".to_string(),
    d: "4".to_string(),
    e: "5".to_string(),
});
```

#### `child` meta

The `child` meta causes the field to be mapped to a child element of the
element.

| Key | Value type | Description |
| --- | --- | --- |
| `default` | flag | If present, an absent child will substitute the default value instead of raising an error. |

The field's type must implement [`FromXml`] in order to derive `FromXml` and
[`AsXml`] in order to derive `AsXml`.

If `default` is specified and the child is absent in the source, the value
is generated using [`std::default::Default`], requiring the field type to
implement the `Default` trait for a `FromXml` derivation. `default` has no
influence on `AsXml`.

##### Example

```rust
# use xso::FromXml;
#[derive(FromXml, Debug, PartialEq)]
#[xml(namespace = "urn:example", name = "child")]
struct Child {
    #[xml(attribute = "some-attr")]
    some_attr: String,
}

#[derive(FromXml, Debug, PartialEq)]
#[xml(namespace = "urn:example", name = "parent")]
struct Parent {
    #[xml(attribute)]
    foo: String,

    #[xml(child)]
    bar: Child,
}

let parent: Parent = xso::from_bytes(b"<parent
    xmlns='urn:example'
    foo='hello world!'
><child some-attr='within'/></parent>").unwrap();
assert_eq!(parent, Parent {
    foo: "hello world!".to_owned(),
    bar: Child { some_attr: "within".to_owned() },
});
```

#### `text` meta

The `text` meta causes the field to be mapped to the text content of the
element.

| Key | Value type | Description |
| --- | --- | --- |
| `codec` | *type* | Optional [`TextCodec`] implementation which is used to encode or decode the field. |

If `codec` is given, the given `codec` must implement
[`TextCodec<T>`][`TextCodec`] where `T` is the type of the field.

If `codec` is *not* given, the field's type must implement [`FromXmlText`] for
`FromXml` and for `AsXml`, the field's type must implement [`AsXmlText`].

The `text` meta also supports a shorthand syntax, `#[xml(text = ..)]`, where
the value is treated as the value for the `codec` key (with optional prefix as
described above, and unnamespaced otherwise).

Only a single field per struct may be annotated with `#[xml(text)]` at a time,
to avoid parsing ambiguities. This is also true if only `AsXml` is derived on
a field, for consistency.

##### Example without codec

```rust
# use xso::FromXml;
#[derive(FromXml, Debug, PartialEq)]
#[xml(namespace = "urn:example", name = "foo")]
struct Foo {
    #[xml(text)]
    a: String,
};

let foo: Foo = xso::from_bytes(b"<foo xmlns='urn:example'>hello</foo>").unwrap();
assert_eq!(foo, Foo {
    a: "hello".to_string(),
});
```

##### Example with codec

```rust
# use xso::FromXml;
#[derive(FromXml, Debug, PartialEq)]
#[xml(namespace = "urn:example", name = "foo")]
struct Foo {
    #[xml(text = xso::text::EmptyAsNone)]
    a: Option<String>,
};

let foo: Foo = xso::from_bytes(b"<foo xmlns='urn:example'/>").unwrap();
assert_eq!(foo, Foo {
    a: None,
});
```
