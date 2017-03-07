extern crate time;
extern crate rustc_serialize;
extern crate docopt;

extern crate annovate;

use std::cmp::max;
use std::path::Path;
use std::fs::{DirBuilder,read_dir};
use std::collections::HashSet;
use std::io::{stderr,Write};

use docopt::Docopt;

use annovate::{Annovate, Annotation, AnnoContainer};

//TODO add support for tap completion as descripted on docopt-rs homepage
//TODO try out rustfmt
//TODO maybe add rename function?
//TODO maybe read metadata from the actual files themselves? Search for annovate tokens in plaintext code files

const USAGE: &'static str = "
Annovate - manage your files' metadata

Usage:
  anno help
  anno [options] new <dirname>
  anno [options] query <filename> [<key>...]
  anno [options] query-dir [<key>...]
  anno [options] put <filename> [<key> <value>]...
  anno [options] put-batch <key> <value> [<filename>...]
  anno [options] put-dir [<key> <value>]...
  anno [options] list [<key>]
  anno [options] get <filename> <key>
  anno [options] get-dir <key>
  anno [options] copy <filename> <filename2> [<key>...]
  anno [options] rm-file-key <filename> [<key>...]
  anno [options] rm-dir-key [<key>...]
  anno [options] drop-file [<filename>...]
  anno [options] report

Options:
  -a                 Include all metadata entries, including overwritten entries
  -m <meta-file>     Path to the meta file that should be used (default ./.annovate)
  -M <meta-outfile>  Path to output meta file. Defaults to whatever -m is
  -d                 Also consider dotfiles when looking for missing metadata (currently not implemented)
  -c                 Also print context information
  -C <context>       Specify context for metadata
  -1                 Only list the most recent entry for a key
  -h --help          Show this help message

Explanation of subcommands:
  help: Display this help
  new: Create a new directory and put a annovate file into it
  query: List (specific or all) meta-properties of a file
  query-dir: List (specific or all) meta-properties of the directory
  add: Add key-value pairs for a single file
  add-batch: Add one common key-value pair for several files
  add-dir: Add key-value pairs of the directory corresponding to the meta file
  list: Show the value for a specific key for several files (default: description)
  get: Print the value for a single key (and nothing more) for a file
  get-dir: Print the value for a single key (and nothing more) for the directory
  copy: Copy key-value pairs from an existing annotation to a new annotation. Context is `copy from filename`
  rm-file: Remove all annotations for a file that have specific keys
  rm-dir: Remove all annotations for the directory that have specific keys
  drop-file: Remove the metadata of specific files completely
  report: Show an overview of which files in the current directory have (=) or have not (-) metadata and which files do not exist (+)
";

fn report_warning( msg: &str ) {
    let mut stderr = stderr();
    let _ = stderr.write( b"[WARNING] " );
    let _ = stderr.write( msg.as_bytes() );
    let _ = stderr.write( b"\n" );
}

fn report_error( msg: &str ) -> ! {
    use std::process::exit;
    let mut stderr = stderr();
    let _ = stderr.write( b"[ERROR] " );
    let _ = stderr.write( msg.as_bytes() );
    let _ = stderr.write( b"\n" );
    exit( 1 );
}

#[derive(Debug, RustcDecodable)]
#[allow(non_snake_case)]
struct Args {
    cmd_help: bool,
    cmd_new: bool,
    cmd_query: bool,
    cmd_query_dir: bool,
    cmd_put: bool,
    cmd_put_batch: bool,
    cmd_put_dir: bool,
    cmd_list: bool,
    cmd_get: bool,
    cmd_get_dir: bool,
    cmd_report: bool,
    cmd_rm_file_key: bool,
    cmd_rm_dir_key: bool,
    cmd_drop_file: bool,

    arg_dirname: String,
    arg_filename: Vec<String>,
    arg_key: Vec<String>,
    arg_value: Vec<String>,

    flag_a: bool,
    flag_m: String,
    flag_M: String,
    flag_d: bool,
    flag_c: bool,
    flag_C: String,
    flag_h: bool,
    flag_help: bool
}


struct ColumnWidths {
    key: usize,
    value: usize,
    context: usize
}

fn determine_column_widths( container: &AnnoContainer,
                            padding: usize ) -> ColumnWidths {

    let mut result = ColumnWidths{ key: 0, value: 0, context: 0 };
    fn num_chars( string: &str ) -> usize {
        string.chars().count()
    }

    for annotation in container {
        result.key = max( num_chars( annotation.key.as_str() ),
                          result.key );
        result.context = max( num_chars( annotation.context.as_str() ),
                              result.context );

        for line in annotation.value.lines() {
            result.value = max( num_chars( line ), result.value );
        }
    }
    result.key += padding;
    result.value += padding;
    result.context += padding;
    result
}

fn filter_duplicates( container: &AnnoContainer ) -> AnnoContainer {
    let mut result = AnnoContainer::new();
    let mut seen = HashSet::new();
    for anno in container.iter().rev() {
        if seen.contains( &anno.key ) {
            continue;
        }
        seen.insert( &anno.key );
        result.push( anno.clone() );
    }
    result
}

fn display_anno_container( container: &AnnoContainer, with_context: bool, show_duplicates: bool ) {
    let filtered_container: AnnoContainer;
    let container = if show_duplicates {
        container
    } else {
        filtered_container = filter_duplicates( container );
        &filtered_container
    };

    let widths = determine_column_widths( container, 2 );
    for annotation in container {
        display_annotation( annotation, &widths, with_context );
    }
}

fn display_annotation( annotation: &Annotation,
                       widths: &ColumnWidths,
                       with_context: bool ) {

    let dummy_str = String::new();
    let mut value_lines = annotation.value.lines();
    let first_line = value_lines.next().unwrap_or( dummy_str.as_str() );
    print!( "{0:2$}{1:3$}",
            annotation.key,
            first_line,
            widths.key,
            widths.value );

    if with_context {
        println!( "{}", annotation.context );
    } else {
        println!( "" );
    }

    while let Some( line ) = value_lines.next() {
        println!( "{0:1$}{2}", "", widths.key, line );
    }
}

fn main() {
    let args: Args = Docopt::new( USAGE )
        .and_then( |d| d.decode() )
        .unwrap_or_else( |e| e.exit() );

    if args.cmd_help || args.flag_h || args.flag_help {
        println!( "{}", USAGE );
        return;
    }

    let bad_filename = "<bad-filename>".to_string();
    let missing_value = "<missing-value>".to_string();
    let missing_context = "<missing-context>".to_string();

    //handle flags/options
    let meta_file = if args.flag_m != "" {
        args.flag_m
    } else if args.cmd_new {
        format!( "{}/.annotave", args.arg_dirname ).to_string()
    } else {
        ".annovate".to_string()
    };
    let meta_outfile = Path::new( if args.flag_M != "" { &args.flag_M } else { &meta_file } );
    let use_dotfiles = args.flag_d;
    let show_context = args.flag_c;
    let show_duplicates = args.flag_a;

    let context = {
        if args.flag_C != "" {
            args.flag_C.clone()
        } else {
            let now = time::now();
            format!( "annovate program, {}.{}.{} {:02}:{:02}:{:02}",
                                   now.tm_mday,
                                   now.tm_mon + 1,
                                   now.tm_year + 1900,
                                   now.tm_hour,
                                   now.tm_min,
                                   now.tm_sec )
                }
    };
    
    //handle commands

    if args.cmd_new {
        let mut dirbuilder = DirBuilder::new();
        if dirbuilder.recursive( true ).create( args.arg_dirname ).is_err() {
            report_error( "Failed to create new directory" );
        }
        //the annovate file will be created automatically because it does not exist
    }

    let mut anno = match Annovate::new( Path::new( &meta_file ) ) {
        Ok( annotations ) => annotations,
        Err( err ) => { println!( "{}", err ); return; }
    };

    let mut require_write_to_disk = false;

    if args.cmd_new {
        //everything should be done by now
    } else if args.cmd_query || args.cmd_query_dir {
        let annotations = if args.cmd_query {
            let query_file = args.arg_filename.get( 0 ).unwrap(); //getopt ensures that this is not empty
            match anno.get_file_annotations( &query_file ) {
                Some( annos ) => annos,
                None => report_error( "Filename has no annotations" )
            }
        } else {
            anno.get_directory_annotations()
        };

        if args.arg_key.len() == 0 {
            display_anno_container( annotations, show_context, show_duplicates );
        } else {
            let mut annotations_subset = AnnoContainer::new();
            for annotation in annotations {
                if args.arg_key.contains( &annotation.key ) {
                    annotations_subset.push( annotation.clone() );
                }
            }
            display_anno_container( &annotations_subset, show_context, show_duplicates );
        }
    } else if args.cmd_put {
        let file_with_new_data = args.arg_filename.get( 0 ).unwrap(); //getopt ensures that this is not empty
        let pairs = args.arg_key.iter().zip( args.arg_value );
        for ( key, value ) in pairs {
            anno.add_file_annotation( file_with_new_data,
                                      Annotation::new( key.clone(),
                                                       value,
                                                       context.clone() ) );
        }
        require_write_to_disk = true;
    } else if args.cmd_put_batch {
        let key = args.arg_key.get( 0 ).unwrap(); //getopt ensures that this is not empty
        let value = args.arg_value.get( 0 ).unwrap(); //getopt ensures that this is not empty
        for filename in args.arg_filename {
            let annotation = Annotation::new( key.clone(),
                                              value.clone(),
                                              context.clone() );
            anno.add_file_annotation( &filename, annotation );
        }
        require_write_to_disk = true;
    } else if args.cmd_put_dir {
        let pairs = args.arg_key.iter().zip( args.arg_value );
        for ( key, value ) in pairs {
            anno.add_directory_annotation( Annotation::new( key.clone(),
                                                            value,
                                                            context.clone() ) );
        }
        require_write_to_disk = true;
    } else if args.cmd_list {
        let default_key = "description".to_string();
        let key = args.arg_key.get( 0 ).unwrap_or( &default_key );
        let mut annotations = AnnoContainer::new();
        for filename in anno.get_files() {
            if !use_dotfiles && filename.starts_with( "." ) {
                continue
            }
            let mut entry_found = false;
            for annotation in anno.get_file_annotations( &filename ).unwrap() { //filename exists because it comes from .get_files()
                if annotation.key == *key {
                    entry_found = true;
                    annotations.push( Annotation::new( filename.clone(), //I am cheating here and use the filename as the key so that I do not need to write extra code for printing the file names
                                                       annotation.value.clone(),
                                                       annotation.context.clone() ) );
                }
            }
            if !entry_found {
                annotations.push( Annotation::new( filename.clone(),
                                                   missing_value.clone(),
                                                   missing_context.clone() ) );
            }
        }
        //TODO add fancy ANSI codes (underline), also add a flag to disable these things and the headers
        annotations.push( Annotation::new( "Filename".to_string(), key.clone(), "Context".to_string() ) ); //header line
        display_anno_container( &annotations, show_context, show_duplicates );
    } else if args.cmd_get || args.cmd_get_dir {
        let key = args.arg_key.get( 0 ).unwrap(); //getopt takes care of non-empty vector

        let annotations = if args.cmd_get {
            let filename = args.arg_filename.get( 0 ).unwrap(); //unwrap takes care of non-empty vector
            match anno.get_file_annotations( filename ) {
                Some( annos ) => annos,
                None => report_error( "Filename has no metadata" )
            }
        } else {
            anno.get_directory_annotations()
        };

        for annotation in annotations {
            if annotation.key == *key {
                println!( "{}", annotation.value );
                if !show_duplicates {
                    break
                }
            }
        }
    } else if args.cmd_report {
        let mut meta_filenames = HashSet::new();
        for filename in anno.get_files() {
            meta_filenames.insert( filename ); //I wonder if there is a more elegant way
        }

        let entries = match read_dir( Path::new( "." ) ) {
            Ok( files ) => {
                files.map( |f| f.unwrap() //TODO find a clean solution to IO error
                                .file_name()
                                .into_string()
                                .unwrap_or( bad_filename.clone() ) )
            },
            Err( e ) => {
                let msg = format!( "Failed to read directory: {}", e );
                report_error( &msg );
            }
        };

        let mut real_filenames = HashSet::new();

        for entry in entries {
            real_filenames.insert( entry );
        }

        for common in real_filenames.intersection( &meta_filenames ) {
            println!( "= {}", common );
        }

        for meta_exclusive in meta_filenames.difference( &real_filenames ) {
            println!( "+ {}", meta_exclusive );
        }

        for real_missing in real_filenames.difference( &meta_filenames ) {
            println!( "- {}", real_missing );
        }

    } else if args.cmd_rm_file_key {
        let filename = args.arg_filename.get( 0 ).expect( "GetOpt has failed to require the argument <filename>" );
        for key in args.arg_key {
            if !anno.remove_file_annotation_entries( filename, &key ) {
                let msg = format!( "No matching entries found for key `{}`", key  );
                report_warning( &msg );
            }
        }
        require_write_to_disk = true;
    } else if args.cmd_rm_dir_key {
        for key in args.arg_key {
            if !anno.remove_directory_annotation_entries( &key ) {
                let msg = format!( "No matching entries found for key `{}`", key  );
                report_warning( &msg );
            }
        }
        require_write_to_disk = true;
    } else if args.cmd_drop_file {
        for file in args.arg_filename {
            if !anno.drop_file_annotations( &file ) {
                let msg = format!( "File is not in annotations: {}", file );
                report_warning( &msg );
            }
        }
        require_write_to_disk = true;
    } else {
        assert!( false ); //docopt should have caught any other case
    }

    if require_write_to_disk {
        if anno.save_as( meta_outfile ).is_err() {
            stderr().write( b"[FATAL] Failed to write annovate file to disk\n" ).unwrap();
        }
    }
}
