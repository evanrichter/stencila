# Audio Object

**An audio file**

This type is an implementation of [schema:AudioObject](https://schema.org/AudioObject).

## Properties

| Name           | `@id`                                                                 | Type                                                                                                 | Description                                                                                                             | Inherited from                  |
| -------------- | --------------------------------------------------------------------- | ---------------------------------------------------------------------------------------------------- | ----------------------------------------------------------------------------------------------------------------------- | ------------------------------- |
| **contentUrl** | [schema:contentUrl](https://schema.org/contentUrl)                    | string                                                                                               | URL for the actual bytes of the media object, for example the image file or video file. See note [1](#notes).           | [MediaObject](MediaObject.md)   |
| about          | [schema:about](https://schema.org/about)                              | Array of [ThingTypes](ThingTypes.md)                                                                 | The subject matter of the content. See note [2](#notes).                                                                | [CreativeWork](CreativeWork.md) |
| alternateNames | [schema:alternateName](https://schema.org/alternateName)              | Array of string                                                                                      | Alternate names (aliases) for the item.                                                                                 | [Thing](Thing.md)               |
| authors        | [schema:author](https://schema.org/author)                            | Parser 'scsi' _and_ Array of ([Person](Person.md) _or_ [Organization](Organization.md))              | The authors of this creative work.                                                                                      | [CreativeWork](CreativeWork.md) |
| bitrate        | [schema:bitrate](https://schema.org/bitrate)                          | number                                                                                               | Bitrate in megabits per second (Mbit/s, Mb/s, Mbps).                                                                    | [MediaObject](MediaObject.md)   |
| caption        | [schema:caption](https://schema.org/caption)                          | string                                                                                               | The caption for this audio recording.                                                                                   | [AudioObject](AudioObject.md)   |
| comments       | [schema:comment](https://schema.org/comment)                          | Array of [Comment](Comment.md)                                                                       | Comments about this creative work.                                                                                      | [CreativeWork](CreativeWork.md) |
| content        | [stencila:content](https://schema.stenci.la/content.jsonld)           | Array of [Node](Node.md) _or_ string                                                                 | The structured content of this creative work c.f. property `text`.                                                      | [CreativeWork](CreativeWork.md) |
| contentSize    | [schema:contentSize](https://schema.org/contentSize)                  | number                                                                                               | File size in megabits (Mbit, Mb).                                                                                       | [MediaObject](MediaObject.md)   |
| dateAccepted   | [stencila:dateAccepted](https://schema.stenci.la/dateAccepted.jsonld) | [Date](Date.md)                                                                                      | Date/time of acceptance. See note [3](#notes).                                                                          | [CreativeWork](CreativeWork.md) |
| dateCreated    | [schema:dateCreated](https://schema.org/dateCreated)                  | [Date](Date.md)                                                                                      | Date/time of creation.                                                                                                  | [CreativeWork](CreativeWork.md) |
| dateModified   | [schema:dateModified](https://schema.org/dateModified)                | [Date](Date.md)                                                                                      | Date/time of most recent modification.                                                                                  | [CreativeWork](CreativeWork.md) |
| datePublished  | [schema:datePublished](https://schema.org/datePublished)              | [Date](Date.md)                                                                                      | Date of first publication.                                                                                              | [CreativeWork](CreativeWork.md) |
| dateReceived   | [schema:dateReceived](https://schema.org/dateReceived)                | [Date](Date.md)                                                                                      | Date/time that work was received.                                                                                       | [CreativeWork](CreativeWork.md) |
| description    | [schema:description](https://schema.org/description)                  | Array of [BlockContent](BlockContent.md) _or_ Array of [InlineContent](InlineContent.md) _or_ string | A description of the item. See note [4](#notes).                                                                        | [Thing](Thing.md)               |
| editors        | [schema:editor](https://schema.org/editor)                            | Array of [Person](Person.md)                                                                         | People who edited the `CreativeWork`.                                                                                   | [CreativeWork](CreativeWork.md) |
| embedUrl       | [schema:embedUrl](https://schema.org/embedUrl)                        | string                                                                                               | URL that can be used to embed the media on a web page via a specific media player.                                      | [MediaObject](MediaObject.md)   |
| fundedBy       | [stencila:fundedBy](https://schema.stenci.la/fundedBy.jsonld)         | Array of ([Grant](Grant.md) _or_ [MonetaryGrant](MonetaryGrant.md))                                  | Grants that funded the `CreativeWork`; reverse of `fundedItems`. See note [5](#notes).                                  | [CreativeWork](CreativeWork.md) |
| funders        | [schema:funder](https://schema.org/funder)                            | Array of ([Person](Person.md) _or_ [Organization](Organization.md))                                  | People or organizations that funded the `CreativeWork`.                                                                 | [CreativeWork](CreativeWork.md) |
| genre          | [schema:genre](https://schema.org/genre)                              | Parser 'csi' _and_ Array of string                                                                   | Genre of the creative work, broadcast channel or group.                                                                 | [CreativeWork](CreativeWork.md) |
| id             | [schema:id](https://schema.org/id)                                    | string                                                                                               | The identifier for this item.                                                                                           | [Entity](Entity.md)             |
| identifiers    | [schema:identifier](https://schema.org/identifier)                    | Array of ([PropertyValue](PropertyValue.md) _or_ string)                                             | Any kind of identifier for any kind of Thing. See note [6](#notes).                                                     | [Thing](Thing.md)               |
| images         | [schema:image](https://schema.org/image)                              | Array of ([ImageObject](ImageObject.md) _or_ Format 'uri')                                           | Images of the item.                                                                                                     | [Thing](Thing.md)               |
| isPartOf       | [schema:isPartOf](https://schema.org/isPartOf)                        | [CreativeWorkTypes](CreativeWorkTypes.md)                                                            | An item or other CreativeWork that this CreativeWork is a part of.                                                      | [CreativeWork](CreativeWork.md) |
| keywords       | [schema:keywords](https://schema.org/keywords)                        | Parser 'csi' _and_ Array of string                                                                   | Keywords or tags used to describe this content. Multiple entries in a keywords list are typically delimited by commas.  | [CreativeWork](CreativeWork.md) |
| licenses       | [schema:license](https://schema.org/license)                          | Array of ([CreativeWorkTypes](CreativeWorkTypes.md) _or_ Format 'uri')                               | License documents that applies to this content, typically indicated by URL.                                             | [CreativeWork](CreativeWork.md) |
| maintainers    | [schema:maintainer](https://schema.org/maintainer)                    | Array of ([Person](Person.md) _or_ [Organization](Organization.md))                                  | The people or organizations who maintain this CreativeWork. See note [7](#notes).                                       | [CreativeWork](CreativeWork.md) |
| mediaType      | [schema:encodingFormat](https://schema.org/encodingFormat)            | string                                                                                               | IANA media type (MIME type). See note [8](#notes).                                                                      | [MediaObject](MediaObject.md)   |
| meta           | [stencila:meta](https://schema.stenci.la/meta.jsonld)                 | object                                                                                               | Metadata associated with this item.                                                                                     | [Entity](Entity.md)             |
| name           | [schema:name](https://schema.org/name)                                | string                                                                                               | The name of the item.                                                                                                   | [Thing](Thing.md)               |
| parts          | [schema:hasParts](https://schema.org/hasParts)                        | Array of [CreativeWorkTypes](CreativeWorkTypes.md)                                                   | Elements of the collection which can be a variety of different elements, such as Articles, Datatables, Tables and more. | [CreativeWork](CreativeWork.md) |
| publisher      | [schema:publisher](https://schema.org/publisher)                      | [Person](Person.md) _or_ [Organization](Organization.md)                                             | A publisher of the CreativeWork.                                                                                        | [CreativeWork](CreativeWork.md) |
| references     | [schema:citation](https://schema.org/citation)                        | Array of ([CreativeWorkTypes](CreativeWorkTypes.md) _or_ string)                                     | References to other creative works, such as another publication, web page, scholarly article, etc.                      | [CreativeWork](CreativeWork.md) |
| text           | [schema:text](https://schema.org/text)                                | string                                                                                               | The textual content of this creative work.                                                                              | [CreativeWork](CreativeWork.md) |
| title          | [schema:headline](https://schema.org/headline)                        | Array of [InlineContent](InlineContent.md) _or_ string                                               | The title of the creative work. See note [9](#notes).                                                                   | [CreativeWork](CreativeWork.md) |
| transcript     | [schema:transcript](https://schema.org/transcript)                    | string                                                                                               | The transcript of this audio recording.                                                                                 | [AudioObject](AudioObject.md)   |
| url            | [schema:url](https://schema.org/url)                                  | Format 'uri'                                                                                         | The URL of the item.                                                                                                    | [Thing](Thing.md)               |
| version        | [schema:version](https://schema.org/version)                          | string _or_ number                                                                                   | The version of the creative work. See note [10](#notes).                                                                | [CreativeWork](CreativeWork.md) |

## Notes

1. **contentUrl** : To allow for file paths, including possibly erroneous paths, niether `format` or `pattern` keywords are used here, or on `embedUrl`.
2. **about** : Consistent with https://schema.org/about, this property allows for linking to one of more `Thing` nodes. This could for example include a `Person` (e.g for a bibliography) or a `DefinedTerm` (e.g. for subject areas the creative work relates to).
3. **dateAccepted** : This is not yet a schema.org property but the term is used [in Dublin Core](http://purl.org/dc/terms/dateAccepted).
4. **description** : Allows for the description to be an array of nodes (e.g. an array of inline content, or a couple of paragraphs), or a string. The `minItems` restriction avoids a string being coerced into an array with a single string item.
5. **fundedBy** : This follows the proposal [here](https://github.com/schemaorg/schemaorg/issues/2258) for a property that is the reverse of `fundedItems`. It is an any because a `CreativeWork` may have been funded through more than one `Grant`.
6. **identifiers** : Some identifiers have specific properties e.g the `issn` property for the `Periodical` type. These should be used in preference to this property which is intended for identifiers that do not yet have a specific property. Identifiers can be represented as strings, but using a `PropertyValue` will usually be better because it allows for `propertyID` (i.e. the type of identifier).
7. **maintainers** : A maintainer of a Dataset, SoftwareApplication, or other CreativeWork. A maintainer is a Person or Organization that manages contributions to, and/or publication of, some (typically complex) artifact. It is common for distributions of software and data to be based on "upstream" sources. When maintainer is applied to a specific version of something e.g. a particular version or packaging of a Dataset, it is always possible that the upstream source has a different maintainer. The isBasedOn property can be used to indicate such relationships between datasets to make the different maintenance roles clear. Similarly in the case of software, a package may have dedicated maintainers working on integration into software distributions such as Ubuntu, as well as upstream maintainers of the underlying work.
8. **mediaType** : This property is more restrictive than https://schema.org/encodingFormat which expects either Text or URL for this property. This schema expects a media type e.g. `text/html` as listed at http://www.iana.org/assignments/media-types/media-types.xhtml.
9. **title** : Allows for the title to include inline content (e.g `Strong`, `Math`) or a string. The title can not be block content e.g `Paragraph`. The `minItems` restriction avoids a string being coerced into an array with a single string item.
10. **version** : In this case `string` is listed as an alternative before `number` to avoid semantic version numbers e.g. `1.0` being parsed, and subsequently encoded, as `1` thereby resulting in loss of information.

## Examples

```json
{
  "type": "AudioObject",
  "caption": "Example Audio File",
  "contentSize": 54321,
  "contentUrl": "http://www.example.com/file.mp3",
  "encodingFormat": "audio/mpeg3",
  "transcript": "This is the transcript for the audio file..."
}
```

## Related

- Parent: [MediaObject](MediaObject.md)
- Descendants: None

## Available as

- [JSON-LD](https://schema.stenci.la/AudioObject.jsonld)
- [JSON Schema](https://schema.stenci.la/v1/AudioObject.schema.json)
- Python [`class AudioObject`](https://stencila.github.io/schema/python/docs/types.html#schema.types.AudioObject)
- TypeScript [`interface AudioObject`](https://stencila.github.io/schema/ts/docs/interfaces/audioobject.html)
- R [`class AudioObject`](https://cran.r-project.org/web/packages/stencilaschema/stencilaschema.pdf)
- Rust [`struct AudioObject`](https://docs.rs/stencila-schema/latest/stencila_schema/struct.AudioObject.html)

## Source

This documentation was generated from [AudioObject.schema.yaml](https://github.com/stencila/stencila/blob/master/schema/schema/AudioObject.schema.yaml).
