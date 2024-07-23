use std::fs::File;
use std::os::unix::prelude::OsStrExt;
use std::path::{Path, PathBuf};
use std::str::FromStr;

use anyhow::bail;
use clap::builder::ArgPredicate;
use clap::Parser;
use serde_json::json;

use crate::benchmark::BenchmarkResults;
use crate::QUANTILES;

#[derive(Debug, Clone)]
struct CommaSeparatedString(Vec<String>);

impl FromStr for CommaSeparatedString {
    type Err = std::convert::Infallible;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(Self(s.split(',').map(Into::into).collect()))
    }
}

#[derive(Parser, Clone)]
pub struct GraphParams {
    /// Run multiple iterations of the benchmark, outputting the results for each of the values of
    /// `--x-axis` provided by `--x-values` into `--graph-results-path`
    #[arg(long, requires_ifs = [(ArgPredicate::IsPresent, "x_axis"), (ArgPredicate::IsPresent, "x_values"), (ArgPredicate::IsPresent, "graph_results_path")])]
    pub graph: bool,

    /// X-axis to vary for the graph. This should be in the format of a (long) command-line
    /// argument accepted by the individual benchmark command, without the `--`; eg:
    /// `target-qps`
    #[arg(long, requires_ifs = [(ArgPredicate::IsPresent, "graph"), (ArgPredicate::IsPresent, "x_values"), (ArgPredicate::IsPresent, "graph_results_path")])]
    x_axis: Option<String>,

    /// List of values to provide to `--x-axis`, represented as a comma-separated string
    #[arg(
        long,
        value_parser = CommaSeparatedString::from_str,
        requires_ifs = [(ArgPredicate::IsPresent, "graph"), (ArgPredicate::IsPresent, "x_axis"), (ArgPredicate::IsPresent, "graph_results_path")]
    )]
    x_values: Option<CommaSeparatedString>,

    /// Flag to indicate if the `--x-axis` is a data generation variable, rather than a
    /// field in the BenchmarkControl struct.
    #[arg(long, requires_ifs = [(ArgPredicate::IsPresent, "graph"), (ArgPredicate::IsPresent, "x_axis"), (ArgPredicate::IsPresent, "x_values"), (ArgPredicate::IsPresent, "graph_results_path")])]
    pub x_axis_is_datagen_var: bool,

    /// File to output graph results to. Currently accepts `.csv` files.
    #[arg(long, requires_ifs = [(ArgPredicate::IsPresent, "graph"), (ArgPredicate::IsPresent, "x_axis"), (ArgPredicate::IsPresent, "x_values")])]
    graph_results_path: Option<PathBuf>,
}

impl GraphParams {
    /// Construct an iterator over individual [`GraphRun`]s for graphing benchmark result outputs.
    ///
    /// # Panics
    ///
    /// Panics if `self.graph` is `false`
    pub fn runs(&self) -> impl Iterator<Item = GraphRun> + '_ {
        let x_axis = self.x_axis.as_ref().unwrap();
        self.x_values.as_ref().unwrap().0.iter().map(|v| GraphRun {
            x_axis: x_axis.clone(),
            x_value: v.clone(),
            x_axis_is_datagen_var: self.x_axis_is_datagen_var,
        })
    }

    /// Construct a writer for writing benchmark results for the graph
    ///
    /// # Panics
    ///
    /// Panics if `self.graph` is `false`
    pub fn results_writer(&self) -> anyhow::Result<GraphResultsWriter> {
        GraphResultsWriter::from_path(self.graph_results_path.as_deref().unwrap())
    }
}

/// Representation of an individual benchmark run for a graph
pub struct GraphRun {
    x_axis: String,
    x_value: String,
    x_axis_is_datagen_var: bool,
}

pub enum ArgOverride {
    CliArgs(Box<dyn Iterator<Item = String>>),
    Json(serde_json::Value),
}

impl GraphRun {
    /// Convert this graph run into a list of command-line arguments that can be passed to
    /// [`crate::Benchmark::update_from`]
    pub fn as_args(&self) -> ArgOverride {
        if self.x_axis_is_datagen_var {
            ArgOverride::Json(json!({ self.x_axis.clone(): self.x_value.clone() }))
        } else {
            ArgOverride::CliArgs(Box::new(
                vec![
                    "benchmarks".to_owned(),
                    format!("--{}", &self.x_axis),
                    self.x_value.clone(),
                ]
                .into_iter(),
            ))
        }
    }

    /// The individual x value for this graph run
    pub fn x_value(&self) -> &str {
        self.x_value.as_ref()
    }
}

/// A writer for graph results
pub enum GraphResultsWriter {
    CSV(csv::Writer<File>),
}

impl GraphResultsWriter {
    /// Construct a new [`GraphResultsWriter`] for writing to the given file path, using the file
    /// extension to infer the output format
    pub fn from_path(path: &Path) -> anyhow::Result<Self> {
        match path.extension().map(|s| s.as_bytes()) {
            Some(b"csv") => Ok(Self::CSV(csv::WriterBuilder::new().from_path(path)?)),
            Some(b"png") => bail!("PNG output not yet implemented"),
            Some(ext) => bail!(
                "Unsupported extension for --graph-results-path: .{}",
                String::from_utf8_lossy(ext)
            ),
            None => bail!("Could not determine output file format from --graph-results-path"),
        }
    }

    /// Write an individual benchmark result to this graph results writer
    pub fn write_result(&mut self, x_value: &str, results: BenchmarkResults) -> anyhow::Result<()> {
        match self {
            GraphResultsWriter::CSV(csv) => {
                let mut result_values = results.results.iter().collect::<Vec<_>>();
                result_values.sort_by(|(k1, _), (k2, _)| k1.cmp(k2));
                // TODO: headers!
                let mut row = vec![x_value.to_owned()];
                row.extend(result_values.into_iter().flat_map(|(_, data)| {
                    let hist = data.to_histogram(0.0, 1.0);
                    let samples = hist.len();
                    let min = hist.min();
                    let max = hist.max();
                    let mean = hist.mean();
                    vec![
                        samples.to_string(),
                        min.to_string(),
                        max.to_string(),
                        mean.to_string(),
                    ]
                    .into_iter()
                    .chain(
                        QUANTILES.iter().map(move |(_, quantile)| {
                            hist.value_at_quantile(*quantile).to_string()
                        }),
                    )
                }));
                csv.write_record(row)?;
            }
        }

        Ok(())
    }
}
