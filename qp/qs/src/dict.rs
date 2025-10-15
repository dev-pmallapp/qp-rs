//! QS Dictionary Macros
//!
//! Macros for generating dictionary records (symbolic name mappings)

/// Generate object dictionary record
#[macro_export]
macro_rules! qs_obj_dict {
    ($obj:expr, $name:expr) => {
        if $crate::begin($crate::QSRecordType::QS_OBJ_DICT, 0) {
            $crate::obj_ptr($obj);
            $crate::str($name);
            $crate::end();
        }
    };
}

/// Generate function dictionary record
#[macro_export]
macro_rules! qs_fun_dict {
    ($fun:expr, $name:expr) => {
        if $crate::begin($crate::QSRecordType::QS_FUN_DICT, 0) {
            $crate::fun_ptr($fun);
            $crate::str($name);
            $crate::end();
        }
    };
}

/// Generate signal dictionary record
#[macro_export]
macro_rules! qs_sig_dict {
    ($sig:expr, $name:expr) => {
        if $crate::begin($crate::QSRecordType::QS_SIG_DICT, 0) {
            $crate::signal($sig);
            $crate::str($name);
            $crate::end();
        }
    };
}

/// Generate user record dictionary
#[macro_export]
macro_rules! qs_usr_dict {
    ($rec:expr, $name:expr) => {
        if $crate::begin($crate::QSRecordType::QS_USR_DICT, 0) {
            $crate::u8($rec);
            $crate::str($name);
            $crate::end();
        }
    };
}

/// Generate enumeration dictionary record
#[macro_export]
macro_rules! qs_enum_dict {
    ($type_name:expr, $value:expr, $name:expr) => {
        if $crate::begin($crate::QSRecordType::QS_ENUM_DICT, 0) {
            $crate::str($type_name);
            $crate::u32($value);
            $crate::str($name);
            $crate::end();
        }
    };
}
