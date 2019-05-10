Current structure:
Header
NumericData
  len
  data_type
DataElement
assert
parse_header
parse_next_data_element
ceil_to_multiple
ArrayFlags
DataType
ArrayType
  numeric_data_type
Dimensions
DataElementTag
parse_data_element_tag
parse_array_name_sublement
parse_dimensions_array_subelement
parse_array_flags_subelement
parse_matrix_data_element
numeric_data_types_are_compatible
parse_numeric_subelement
parse_compressed_data_element
RowIndex
ColumnShift
parse_numeric_matrix_subelements
parse_sparse_matrix_subelements
parse_row_index_array_subelement
parse_column_index_array_subelement
replace_context_sice
replace_error_slice
parse_unsupported_data_element
ParseResult
parse_all

Proposed structure
parse
 - ParseResult
 - parse_all

parse::helpers
 replace_context_slice
 replace_err_slice
 numeric_data_types_are_compatible
 assert
 ceil_to_multiple

parse::simple data
 - Header
   - parse_header
 - NumericData
     - len
     - data_type
   - parse_numeric_subelement
 - DataType
 - DataElementTag
   - parse_data_element_tag
 - DataElement -> use ArrayFlags, Dimensions, NumericData
   - parse_next_data_element
   - parse_compressed_data_element
   - parse_unsupported_data_element

parse::arrays
 - ArrayType
   - numeric_data_type
 - ArrayFlags
   - parse_array_flag_subelement
 - Dimensions
   - parse_dimension_array_subelement
 - parse_array_name_subelement
 - parse_matrix_data_element

parse::array::numeric
 - parse_numeric_matrix_subelements

parse::array::sparse
 - RowIndex
   - parse_row_index_array_subelement
 - ColumnShift
   - parse_column_index_array_subelement
 - parse_sparse_matrix_subelements

parse::array::structure
 - FieldNameLength
   - parse_field_name_length_subelement
 - FieldName
   - parse_field_name
 - Fields
 - parse_structure_matrix_subelements