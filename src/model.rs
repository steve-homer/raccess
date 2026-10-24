//! Serde mirrors of the jetdb types we serialise.
//!
//! jetdb has no serde support, so every type here exists to give one of its
//! structs a stable JSON shape. Field order is the serialised order, so output
//! stays diffable across runs.

use jetdb::{
    BlobProperty, BlobValue, ColumnDef, ColumnType, ControlProperties, FormProperties,
    IndexColumnOrder, IndexDef, MacroArgument, MacroBranch, MacroDef, MacroSource, MacroStatement,
    MacroXmlElement, QueryDef, Relationship, TableDef, column_flags, index_flags, index_type,
    query_to_sql, relationship_flags,
};
use serde::Serialize;

// ---------------------------------------------------------------------------
// manifest.json
// ---------------------------------------------------------------------------

#[derive(Serialize, Clone, Copy, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Status {
    Ok,
    Partial,
    Failed,
}

#[derive(Serialize)]
pub struct Manifest {
    pub raccess_version: &'static str,
    pub jetdb_version: &'static str,
    pub extracted_at: String,
    pub source: Source,
    pub objects: Vec<ObjectRecord>,
}

#[derive(Serialize)]
pub struct Source {
    pub path: String,
    pub format: String,
    pub bytes: u64,
    pub sha256: String,
    pub code_page: u16,
    pub lang_id: u16,
}

#[derive(Serialize)]
pub struct ObjectRecord {
    pub kind: &'static str,
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub file: Option<String>,
    pub status: Status,
    /// Row count the table definition declares.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rows: Option<usize>,
    /// Rows jetdb could not decode, and so are absent from the CSV.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub skipped_rows: Option<usize>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub warnings: Vec<String>,
}

impl ObjectRecord {
    pub fn ok(kind: &'static str, name: impl Into<String>, file: impl Into<String>) -> Self {
        Self {
            kind,
            name: name.into(),
            file: Some(file.into()),
            status: Status::Ok,
            rows: None,
            skipped_rows: None,
            warnings: Vec::new(),
        }
    }

    pub fn failed(kind: &'static str, name: impl Into<String>, why: impl Into<String>) -> Self {
        Self {
            kind,
            name: name.into(),
            file: None,
            status: Status::Failed,
            rows: None,
            skipped_rows: None,
            warnings: vec![why.into()],
        }
    }

    pub fn with_rows(mut self, rows: usize, skipped: usize) -> Self {
        self.rows = Some(rows);
        if skipped > 0 {
            self.skipped_rows = Some(skipped);
            self.warnings.push(format!(
                "{skipped} row(s) could not be decoded and are not in the CSV"
            ));
            self.status = Status::Partial;
        }
        self
    }

    pub fn warn(mut self, why: impl Into<String>) -> Self {
        self.warnings.push(why.into());
        self.status = Status::Partial;
        self
    }
}

// ---------------------------------------------------------------------------
// schema.json
// ---------------------------------------------------------------------------

#[derive(Serialize)]
pub struct Schema {
    pub tables: Vec<SchemaTable>,
    pub relationships: Vec<SchemaRelationship>,
}

#[derive(Serialize)]
pub struct SchemaTable {
    pub name: String,
    pub rows: u32,
    pub system: bool,
    pub columns: Vec<SchemaColumn>,
    pub indexes: Vec<SchemaIndex>,
}

#[derive(Serialize)]
pub struct SchemaColumn {
    pub name: String,
    #[serde(rename = "type")]
    pub col_type: String,
    /// Length as Access shows it: characters for Text, bytes otherwise.
    pub size: u16,
    pub ordinal: u16,
    pub nullable: bool,
    pub auto_number: bool,
    pub calculated: bool,
    pub hyperlink: bool,
    /// Only meaningful for Decimal columns; absent otherwise, because jetdb
    /// reports the format's defaults for every other type.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub precision: Option<u8>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub scale: Option<u8>,
}

#[derive(Serialize)]
pub struct SchemaIndex {
    pub name: String,
    pub columns: Vec<SchemaIndexColumn>,
    pub primary: bool,
    pub foreign_key: bool,
    pub unique: bool,
    pub required: bool,
    pub ignore_nulls: bool,
}

#[derive(Serialize)]
pub struct SchemaIndexColumn {
    pub name: String,
    pub descending: bool,
}

#[derive(Serialize)]
pub struct SchemaRelationship {
    pub name: String,
    pub from_table: String,
    pub to_table: String,
    pub columns: Vec<SchemaRelationshipColumn>,
    pub referential_integrity: bool,
    pub cascade_update: bool,
    pub cascade_delete: bool,
}

#[derive(Serialize)]
pub struct SchemaRelationshipColumn {
    pub from: String,
    pub to: String,
}

impl SchemaTable {
    pub fn new(table: &TableDef, system: bool) -> Self {
        Self {
            name: table.name.clone(),
            rows: table.num_rows,
            system,
            columns: {
                let mut columns: Vec<_> = table
                    .columns
                    .iter()
                    .map(|c| SchemaColumn::new(table, c))
                    .collect();
                columns.sort_by_key(|c| c.ordinal);
                columns
            },
            indexes: table
                .indexes
                .iter()
                .map(|i| SchemaIndex::new(table, i))
                .collect(),
        }
    }
}

impl SchemaColumn {
    fn new(table: &TableDef, col: &ColumnDef) -> Self {
        Self {
            name: col.name.clone(),
            col_type: format!("{:?}", col.col_type),
            size: table.shown_size(col),
            ordinal: col.display_index,
            nullable: col.flags & column_flags::NULLABLE != 0,
            auto_number: col.flags & (column_flags::AUTO_LONG | column_flags::AUTO_UUID) != 0,
            calculated: col.is_calculated,
            hyperlink: col.flags & column_flags::HYPERLINK != 0,
            precision: (col.col_type == ColumnType::Numeric).then_some(col.precision),
            scale: (col.col_type == ColumnType::Numeric).then_some(col.scale),
        }
    }
}

impl SchemaIndex {
    fn new(table: &TableDef, index: &IndexDef) -> Self {
        Self {
            name: index.name.clone(),
            columns: index
                .columns
                .iter()
                .map(|ic| SchemaIndexColumn {
                    name: table
                        .columns
                        .iter()
                        .find(|c| c.col_num == ic.col_num)
                        .map_or_else(|| format!("#{}", ic.col_num), |c| c.name.clone()),
                    descending: ic.order == IndexColumnOrder::Descending,
                })
                .collect(),
            primary: index.index_type == index_type::PRIMARY,
            foreign_key: index.index_type == index_type::FOREIGN_KEY,
            unique: index.flags & index_flags::UNIQUE != 0,
            required: index.flags & index_flags::REQUIRED != 0,
            ignore_nulls: index.flags & index_flags::IGNORE_NULLS != 0,
        }
    }
}

impl SchemaRelationship {
    pub fn new(rel: &Relationship) -> Self {
        Self {
            name: rel.name.clone(),
            from_table: rel.from_table.clone(),
            to_table: rel.to_table.clone(),
            columns: rel
                .columns
                .iter()
                .map(|c| SchemaRelationshipColumn {
                    from: c.from_column.clone(),
                    to: c.to_column.clone(),
                })
                .collect(),
            referential_integrity: rel.flags & relationship_flags::NO_REFERENTIAL_INTEGRITY == 0,
            cascade_update: rel.flags & relationship_flags::CASCADE_UPDATE != 0,
            cascade_delete: rel.flags & relationship_flags::CASCADE_DELETE != 0,
        }
    }
}

// ---------------------------------------------------------------------------
// queries/
// ---------------------------------------------------------------------------

#[derive(Serialize)]
pub struct Query {
    pub name: String,
    #[serde(rename = "type")]
    pub query_type: String,
    pub sql: String,
    /// Some MSysQueries rows could not be read, so `sql` may be missing clauses.
    pub incomplete: bool,
}

impl Query {
    pub fn new(query: &QueryDef) -> Self {
        Self {
            name: query.name.clone(),
            query_type: format!("{:?}", query.query_type),
            sql: query_to_sql(query),
            incomplete: query.incomplete,
        }
    }
}

// ---------------------------------------------------------------------------
// forms/ and reports/
// ---------------------------------------------------------------------------

#[derive(Serialize)]
pub struct Form {
    pub name: String,
    pub kind: String,
    pub properties: Vec<Property>,
    pub controls: Vec<Control>,
}

#[derive(Serialize)]
pub struct Control {
    pub name: String,
    pub type_code: u16,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub type_name: Option<String>,
    pub properties: Vec<Property>,
}

#[derive(Serialize)]
pub struct Property {
    /// Access's name for the property, absent when the id is not a known one.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<&'static str>,
    pub id: u16,
    pub value: PropertyValue,
}

#[derive(Serialize)]
#[serde(untagged)]
pub enum PropertyValue {
    Bool(bool),
    Int(i64),
    Float(f64),
    Text(String),
    /// Binary, as lowercase hex.
    Bytes {
        hex: String,
    },
    /// A Yes/No property stored as "whatever is not the default".
    NotDefault {
        not_default: bool,
    },
}

impl Form {
    pub fn new(props: &FormProperties) -> Self {
        Self {
            name: props.form_name.clone(),
            kind: format!("{:?}", props.object_type),
            properties: props.properties.iter().map(Property::new).collect(),
            controls: props.controls.iter().map(Control::new).collect(),
        }
    }
}

impl Control {
    fn new(control: &ControlProperties) -> Self {
        Self {
            name: control.name.clone(),
            type_code: control.type_code,
            type_name: jetdb::control_type_name(control.type_code).map(str::to_owned),
            properties: control.properties.iter().map(Property::new).collect(),
        }
    }
}

impl Property {
    fn new(prop: &BlobProperty) -> Self {
        Self {
            name: prop.name(),
            id: prop.prop_id,
            value: match &prop.value {
                BlobValue::Bool(b) => PropertyValue::Bool(*b),
                BlobValue::NotDefault => PropertyValue::NotDefault { not_default: true },
                BlobValue::Short(n) => PropertyValue::Int(i64::from(*n)),
                BlobValue::Long(n) => PropertyValue::Int(i64::from(*n)),
                BlobValue::Double(f) => PropertyValue::Float(*f),
                BlobValue::Guid(s) | BlobValue::Text(s) => PropertyValue::Text(s.clone()),
                BlobValue::Binary(b) => PropertyValue::Bytes { hex: hex(b) },
            },
        }
    }
}

pub fn hex(bytes: &[u8]) -> String {
    use std::fmt::Write;
    bytes.iter().fold(String::new(), |mut s, b| {
        let _ = write!(s, "{b:02x}");
        s
    })
}

// ---------------------------------------------------------------------------
// macros/
// ---------------------------------------------------------------------------

#[derive(Serialize)]
pub struct Macro {
    pub name: String,
    pub source: MacroOrigin,
    pub statements: Vec<Statement>,
}

#[derive(Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum MacroOrigin {
    Named {
        name: String,
    },
    Embedded {
        object_kind: String,
        object_name: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        control: Option<String>,
        event: String,
    },
    Data {
        table: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        event: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        name: Option<String>,
        #[serde(skip_serializing_if = "Vec::is_empty")]
        parameters: Vec<Parameter>,
    },
}

#[derive(Serialize)]
pub struct Parameter {
    pub name: String,
    pub attributes: Vec<Argument>,
}

#[derive(Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Statement {
    Action {
        name: String,
        arguments: Vec<Argument>,
    },
    Comment {
        text: String,
    },
    Conditional {
        branches: Vec<Branch>,
    },
    Group {
        description: String,
        statements: Vec<Statement>,
    },
    SubMacro {
        name: String,
        statements: Vec<Statement>,
    },
    DataBlock {
        block: String,
        data: Vec<Argument>,
        statements: Vec<Statement>,
    },
    /// An XML element jetdb does not interpret, kept verbatim.
    Unknown {
        element: String,
    },
}

#[derive(Serialize)]
pub struct Argument {
    pub name: String,
    pub value: String,
}

#[derive(Serialize)]
pub struct Branch {
    /// `None` for the `Else` branch.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub condition: Option<String>,
    pub statements: Vec<Statement>,
}

impl Macro {
    pub fn new(name: &str, def: &MacroDef) -> Self {
        Self {
            name: name.to_owned(),
            source: MacroOrigin::new(&def.source),
            statements: def.statements.iter().map(Statement::new).collect(),
        }
    }
}

impl MacroOrigin {
    fn new(source: &MacroSource) -> Self {
        match source {
            MacroSource::Named { name } => MacroOrigin::Named { name: name.clone() },
            MacroSource::Embedded {
                object_kind,
                object_name,
                control,
                event,
            } => MacroOrigin::Embedded {
                object_kind: format!("{object_kind:?}"),
                object_name: object_name.clone(),
                control: control.clone(),
                event: event.clone(),
            },
            MacroSource::Data {
                table,
                event,
                name,
                parameters,
            } => MacroOrigin::Data {
                table: table.clone(),
                event: event.clone(),
                name: name.clone(),
                parameters: parameters
                    .iter()
                    .map(|p| Parameter {
                        name: p.name.clone(),
                        attributes: p
                            .attributes
                            .iter()
                            .map(|(name, value)| Argument {
                                name: name.clone(),
                                value: value.clone(),
                            })
                            .collect(),
                    })
                    .collect(),
            },
        }
    }
}

impl Statement {
    fn new(statement: &MacroStatement) -> Self {
        match statement {
            MacroStatement::Action { name, arguments } => Statement::Action {
                name: name.clone(),
                arguments: arguments.iter().map(Argument::new).collect(),
            },
            MacroStatement::Comment(text) => Statement::Comment { text: text.clone() },
            MacroStatement::Conditional { branches } => Statement::Conditional {
                branches: branches.iter().map(Branch::new).collect(),
            },
            MacroStatement::Group {
                description,
                statements,
            } => Statement::Group {
                description: description.clone(),
                statements: statements.iter().map(Statement::new).collect(),
            },
            MacroStatement::SubMacro { name, statements } => Statement::SubMacro {
                name: name.clone(),
                statements: statements.iter().map(Statement::new).collect(),
            },
            MacroStatement::DataBlock {
                kind,
                data,
                statements,
            } => Statement::DataBlock {
                block: kind.clone(),
                data: data
                    .iter()
                    .map(|(name, value)| Argument {
                        name: name.clone(),
                        value: value.clone(),
                    })
                    .collect(),
                statements: statements.iter().map(Statement::new).collect(),
            },
            MacroStatement::Unknown(element) => Statement::Unknown {
                element: describe_element(element),
            },
        }
    }
}

impl Argument {
    fn new(argument: &MacroArgument) -> Self {
        Self {
            name: argument.name.clone(),
            value: argument.value.clone(),
        }
    }
}

impl Branch {
    fn new(branch: &MacroBranch) -> Self {
        Self {
            condition: branch.condition.clone(),
            statements: branch.statements.iter().map(Statement::new).collect(),
        }
    }
}

fn describe_element(element: &MacroXmlElement) -> String {
    format!("{element:?}")
}
