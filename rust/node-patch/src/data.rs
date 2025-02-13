use stencila_schema::*;

use super::prelude::*;

impl Patchable for Datatable {
    fn diff(&self, other: &Self, differ: &mut Differ) {
        // TODO: Implement diffing optimized (semantically and computationally) for datatables
        // e.g. `Add` and `Remove` for entire columns and entire rows,
        // `Replace` for individual cells
        differ.replace(other)
    }
}

patchable_struct!(DatatableColumn, name, validator, values);

// Previously we implemented a custom `Patchable` for `Parameter` to ensure that values of
// `default` and `value` fields (which can be any `Node`) meet the requirements of the `validator`.
// However, doing that here causes a lot of inconsistencies, especially in the UI. For that
// reason we revert to using standard struct macro here.
patchable_struct!(
    Parameter,
    name,
    validator,
    default,
    value,
    compile_digest,
    execute_digest,
    execute_required
);

patchable_variants!(
    ValidatorTypes,
    ValidatorTypes::ArrayValidator,
    ValidatorTypes::BooleanValidator,
    ValidatorTypes::ConstantValidator,
    ValidatorTypes::EnumValidator,
    ValidatorTypes::IntegerValidator,
    ValidatorTypes::NumberValidator,
    ValidatorTypes::StringValidator,
    ValidatorTypes::TupleValidator
);
patchable_struct!(ArrayValidator);
patchable_struct!(BooleanValidator);
patchable_struct!(ConstantValidator, value);
patchable_struct!(
    IntegerValidator,
    minimum,
    maximum,
    exclusive_minimum,
    exclusive_maximum,
    multiple_of
);
patchable_struct!(
    NumberValidator,
    minimum,
    maximum,
    exclusive_minimum,
    exclusive_maximum,
    multiple_of
);
patchable_struct!(StringValidator, min_length, max_length, pattern);
patchable_struct!(TupleValidator, items);

// The `EnumValidator` is replaceable because it is to difficult to
// work with fine grained DOM patches to `values` (because they are in a <select>).
// Instead the `parameterValidator` proxy knows how to deal with replacement of
// this type of validator.
replaceable_struct!(EnumValidator, values);
