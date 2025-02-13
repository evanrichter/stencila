pub const IDS: &[(&str, &str)] = &[
    ("Array", "https://schema.stenci.la/Array"),
    ("ArrayValidator", "https://schema.stenci.la/ArrayValidator"),
    ("Article", "https://schema.org/Article"),
    ("alternateNames", "alternateName"),
    ("identifiers", "identifier"),
    ("images", "image"),
    ("authors", "author"),
    ("comments", "comment"),
    ("editors", "editor"),
    ("funders", "funder"),
    ("licenses", "license"),
    ("maintainers", "maintainer"),
    ("parts", "hasParts"),
    ("references", "citation"),
    ("title", "headline"),
    ("AudioObject", "https://schema.org/AudioObject"),
    ("mediaType", "encodingFormat"),
    ("Boolean", "https://schema.org/Boolean"),
    ("BooleanValidator", "https://schema.stenci.la/BooleanValidator"),
    ("Brand", "https://schema.org/Brand"),
    ("reviews", "review"),
    ("Call", "https://schema.stenci.la/Call"),
    ("CallArgument", "https://schema.stenci.la/CallArgument"),
    ("default", "defaultValue"),
    ("isRequired", "valueRequired"),
    ("CitationIntent", "https://schema.stenci.la/CitationIntent"),
    ("Cite", "https://schema.stenci.la/Cite"),
    ("CiteGroup", "https://schema.stenci.la/CiteGroup"),
    ("items", "itemListElement"),
    ("Claim", "https://schema.org/Claim"),
    ("Code", "https://schema.stenci.la/Code"),
    ("CodeBlock", "https://schema.stenci.la/CodeBlock"),
    ("CodeChunk", "https://schema.stenci.la/CodeChunk"),
    ("CodeError", "https://schema.stenci.la/CodeError"),
    ("CodeExecutable", "https://schema.stenci.la/CodeExecutable"),
    ("CodeExpression", "https://schema.stenci.la/CodeExpression"),
    ("CodeFragment", "https://schema.stenci.la/CodeFragment"),
    ("Collection", "https://schema.org/Collection"),
    ("Comment", "https://schema.org/Comment"),
    ("ConstantValidator", "https://schema.stenci.la/ConstantValidator"),
    ("ContactPoint", "https://schema.org/ContactPoint"),
    ("availableLanguages", "availableLanguage"),
    ("emails", "email"),
    ("telephoneNumbers", "telephone"),
    ("CreativeWork", "https://schema.org/CreativeWork"),
    ("Datatable", "https://schema.stenci.la/Datatable"),
    ("DatatableColumn", "https://schema.stenci.la/DatatableColumn"),
    ("Date", "https://schema.org/Date"),
    ("DefinedTerm", "https://schema.org/DefinedTerm"),
    ("Delete", "https://schema.stenci.la/Delete"),
    ("Emphasis", "https://schema.stenci.la/Emphasis"),
    ("Entity", "https://schema.stenci.la/Entity"),
    ("EnumValidator", "https://schema.stenci.la/EnumValidator"),
    ("Enumeration", "https://schema.org/Enumeration"),
    ("ExecuteAuto", "https://schema.stenci.la/ExecuteAuto"),
    ("ExecuteRequired", "https://schema.stenci.la/ExecuteRequired"),
    ("ExecuteStatus", "https://schema.stenci.la/ExecuteStatus"),
    ("Figure", "https://schema.stenci.la/Figure"),
    ("File", "https://schema.stenci.la/File"),
    ("Function", "https://schema.stenci.la/Function"),
    ("Grant", "https://schema.org/Grant"),
    ("fundedItems", "fundedItem"),
    ("sponsors", "sponsor"),
    ("Heading", "https://schema.stenci.la/Heading"),
    ("ImageObject", "https://schema.org/ImageObject"),
    ("Include", "https://schema.stenci.la/Include"),
    ("Integer", "https://schema.org/Integer"),
    ("IntegerValidator", "https://schema.stenci.la/IntegerValidator"),
    ("Link", "https://schema.stenci.la/Link"),
    ("relation", "linkRelationship"),
    ("List", "https://schema.org/ItemList"),
    ("order", "itemListOrder"),
    ("ListItem", "https://schema.org/ListItem"),
    ("Mark", "https://schema.stenci.la/Mark"),
    ("Math", "https://schema.stenci.la/Math"),
    ("MathBlock", "https://schema.stenci.la/MathBlock"),
    ("MathFragment", "https://schema.stenci.la/MathFragment"),
    ("MediaObject", "https://schema.org/MediaObject"),
    ("MonetaryGrant", "https://schema.org/MonetaryGrant"),
    ("amounts", "amount"),
    ("NontextualAnnotation", "https://schema.stenci.la/NontextualAnnotation"),
    ("Note", "https://schema.stenci.la/Note"),
    ("Null", "https://schema.stenci.la/Null"),
    ("Number", "https://schema.org/Number"),
    ("NumberValidator", "https://schema.stenci.la/NumberValidator"),
    ("Object", "https://schema.stenci.la/Object"),
    ("Organization", "https://schema.org/Organization"),
    ("brands", "brand"),
    ("contactPoints", "contactPoint"),
    ("departments", "department"),
    ("members", "member"),
    ("Paragraph", "https://schema.stenci.la/Paragraph"),
    ("Parameter", "https://schema.stenci.la/Parameter"),
    ("Periodical", "https://schema.org/Periodical"),
    ("dateStart", "startDate"),
    ("dateEnd", "endDate"),
    ("issns", "issn"),
    ("Person", "https://schema.org/Person"),
    ("affiliations", "affiliation"),
    ("familyNames", "familyName"),
    ("givenNames", "givenName"),
    ("PostalAddress", "https://schema.org/PostalAddress"),
    ("Product", "https://schema.org/Product"),
    ("PropertyValue", "https://schema.org/PropertyValue"),
    ("PublicationIssue", "https://schema.org/PublicationIssue"),
    ("PublicationVolume", "https://schema.org/PublicationVolume"),
    ("Quote", "https://schema.stenci.la/Quote"),
    ("QuoteBlock", "https://schema.stenci.la/QuoteBlock"),
    ("Review", "https://schema.org/Review"),
    ("SoftwareApplication", "https://schema.org/SoftwareApplication"),
    ("SoftwareEnvironment", "https://schema.stenci.la/SoftwareEnvironment"),
    ("SoftwareSession", "https://schema.stenci.la/SoftwareSession"),
    ("status", "sessionStatus"),
    ("SoftwareSourceCode", "https://schema.org/SoftwareSourceCode"),
    ("targetProducts", "targetProduct"),
    ("Strikeout", "https://schema.stenci.la/Strikeout"),
    ("String", "https://schema.org/Text"),
    ("StringValidator", "https://schema.stenci.la/StringValidator"),
    ("Strong", "https://schema.stenci.la/Strong"),
    ("Subscript", "https://schema.stenci.la/Subscript"),
    ("Superscript", "https://schema.stenci.la/Superscript"),
    ("Table", "https://schema.org/Table"),
    ("TableCell", "https://schema.stenci.la/TableCell"),
    ("TableRow", "https://schema.stenci.la/TableRow"),
    ("ThematicBreak", "https://schema.stenci.la/ThematicBreak"),
    ("Thing", "https://schema.org/Thing"),
    ("TupleValidator", "https://schema.stenci.la/TupleValidator"),
    ("Underline", "https://schema.stenci.la/Underline"),
    ("Validator", "https://schema.stenci.la/Validator"),
    ("Variable", "https://schema.stenci.la/Variable"),
    ("isReadonly", "readonlyValue"),
    ("VideoObject", "https://schema.org/VideoObject"),
    ("VolumeMount", "https://schema.stenci.la/VolumeMount")
];