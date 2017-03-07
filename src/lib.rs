extern crate time;

use std::io::{BufRead, BufReader, Write};
use std::io;
use std::collections::hash_map::HashMap;
use std::path::{Path,PathBuf};
use std::fs::File;
use std::fmt;

#[derive(Clone)]
pub struct Annotation {
    pub key: String,
    pub value: String,
    pub context: String
}

impl Annotation {
    pub fn new( key: String, value: String, context: String ) -> Annotation {
        Annotation { key: key, value: value, context: context }
    }
}

pub type AnnoContainer = Vec<Annotation>;

pub struct Annovate {
    dir: AnnoContainer,
    files: HashMap<String, AnnoContainer>,
    save_changes: bool,
    filename: PathBuf
}

#[derive(Debug)]
pub enum AnnoError {
    ParseError( u64, char ),
    IOError( io::Error )
}

impl fmt::Display for AnnoError {
    fn fmt( &self, f: &mut fmt::Formatter) -> fmt::Result {
        match *self {
            AnnoError::ParseError( line, symbol ) => write!( f, "Invalid token `{}` at the beginning of line {}", symbol, line ),
            AnnoError::IOError( ref ioe ) => write!( f, "IO error: {}", ioe ),
        }
    }
}

impl From<io::Error> for AnnoError {
    fn from( err: io::Error ) -> AnnoError {
        AnnoError::IOError( err )
    }
}

fn test_leader( last_leader: char, legal_chars: &str, current_leader: char, line_no: u64 ) -> Result<(), AnnoError> {
    for c in legal_chars.chars() {
        if c == last_leader {
            return Ok( () )
        }
    }
    Err( AnnoError::ParseError( line_no, current_leader ) )
}

fn extract_line_parts<'a>( line: &'a str ) -> ( char, &'a str ) {
    if line.len() == 0 {
        ( ' ', line )
    } else {
        let leader = line.chars().next().unwrap();
        let rest = line[ 1.. ].trim_right();
        ( leader, rest )
    }
}

fn create_new_annovate_file( filepath: &Path, creation_reason: &str ) -> io::Result<()> {
    let mut new_file = try!( File::create( filepath ) );
    let now = time::now();
    let timestring = format!( "{}.{}.{} {}:{}:{}", now.tm_mday, now.tm_mon + 1, now.tm_year + 1900, now.tm_hour, now.tm_min, now.tm_sec );
    write!( new_file, ">creation time\n={}\n<{}, {}\n", timestring, timestring, creation_reason );
    Ok( try!( new_file.flush() ) )
}

fn parse_annovate_file( filepath: &Path ) -> Result<Annovate, AnnoError> {
    let mut result = Annovate {
        filename: filepath.to_path_buf(),
        dir: vec![],
        files: HashMap::new(),
        save_changes: true
    };

    let fd = match File::open( filepath ) {
        Ok( file_handle ) => file_handle,
        Err( _ ) => {
            try!( create_new_annovate_file( filepath, "new annovate file" ) );
            try!( File::open( filepath ) )
        }
    };
    let reader = BufReader::new( fd );

    let mut work_with_dir_fields = true;
    let mut current_file = String::new();

    let mut current_key = String::new();
    let mut current_value = String::new();

    let mut last_leader = ' '; //dummy value
    let mut line_no = 1u64;
    for line_result in reader.lines() {
        let line = try!( line_result );
        let ( leader, rest ) = extract_line_parts( &line );
        if leader == '@' {
            try!( test_leader( last_leader, "@< ", leader, line_no ) );
            result.files.insert( rest.to_string(), vec![] );
            current_file = rest.to_string();
            work_with_dir_fields = false;
        } else if leader == '>' {
            try!( test_leader( last_leader, "@< ", leader, line_no ) );
            current_key = rest.to_string();
            current_value = String::new();
        } else if leader == '=' {
            try!( test_leader( last_leader, ">=", leader, line_no ) );
            if current_value != "" {
                current_value.push_str( "\n" ); //separate lines with newline
            }
            current_value.push_str( rest );
        } else if leader == '<' {
            try!( test_leader( last_leader, "=>", leader, line_no ) );
            let anno = Annotation{
                key: current_key.clone(),
                value: current_value.clone(),
                context: rest.to_string()
            };

            if work_with_dir_fields { //then fill dir
                result.dir.push( anno );
            } else {
                let mut entry = result.files.get_mut( &current_file ).unwrap();
                entry.push( anno );
            }
        } else {
             return Err( AnnoError::ParseError( line_no, leader ) );
        }
        last_leader = leader;
        line_no += 1;
    }
    if last_leader == '<' {
        Ok( result )
    } else {
         Err( AnnoError::ParseError( line_no, ' ' ) )
    }
}


impl Annovate {
    /// Create new annovation file and return annotation object
    pub fn new( file: &Path ) -> Result<Annovate, AnnoError> {
        parse_annovate_file( file )
    }

    /// Write annovate file to disk
    pub fn save( &self ) -> Result<(), AnnoError> {
        self.save_as( &self.filename )
    }

    pub fn save_as( &self, outfile: &Path ) -> Result<(), AnnoError> {
        let mut file = try!( File::create( outfile ) );

        fn write_annotations( file: &mut File, annotations: &AnnoContainer ) -> Result<(), AnnoError> {
            for anno in annotations {
                try!( write!( file, ">{}\n", anno.key ) );
                for line in anno.value.lines() {
                    try!( write!( file, "={}\n", line ) );
                }
                try!( write!( file, "<{}\n", anno.context ) );
            }
            Ok( () )
        }
        
        try!( write_annotations( &mut file, &self.dir ) );

        for anno_file in self.files.keys() {
            try!( write!( file, "@{}\n", anno_file ) );
            for annotations in self.files.get( anno_file ) {
                try!( write_annotations( &mut file, annotations ) );
            }
        }
        Ok( () )
    }

    /// Get a vector of filenames (copied strings)
    pub fn get_files( &self ) -> Vec<String> {
        let mut result = Vec::new();
        for file in self.files.keys() {
            result.push( file.clone() );
        }
        result
    }

    pub fn get_directory_annotations( &self ) -> &AnnoContainer {
        &self.dir
    }

    pub fn get_file_annotations( &self, filename: &str ) -> Option<&AnnoContainer> {
        self.files.get( filename )
    }

    pub fn add_directory_annotation( &mut self, anno: Annotation ) -> () {
        self.dir.push( anno );
    }

    pub fn remove_directory_annotation_entries( &mut self, key: &str ) -> bool {
        let old_length = self.dir.len();
        self.dir.retain( |x| x.key != key ); //delete all existing annotations with the key
        old_length > self.dir.len() //return true if there was an entry that was removed
    }

    pub fn add_file_annotation( &mut self, filename: &str, anno: Annotation ) -> () {
        let mut vals = self.files.entry( filename.to_string() ).or_insert( AnnoContainer::new() );
        vals.push( anno )
    }

    pub fn remove_file_annotation_entries( &mut self, filename: &str, key: &str ) -> bool {
        let entries = self.files.get_mut( filename );
        match entries {
            Some( vals ) => {
                let old_length = vals.len();
                vals.retain( |x| x.key != key );
                return old_length > vals.len();
            },
            None => return false
        }
    }

    pub fn drop_file_annotations( &mut self, filename: &str ) -> bool {
        self.files.remove( filename ).is_some()
    }
}

//TODO write tests to make it rock solid
#[cfg(test)]
mod tests {
    #[test]
    fn it_works() {
    }
}
