use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(name = "ts-screen")]
#[command(about = "A tool to analyze telescope scheduler database", long_about = None)]
pub struct Cli {
    #[arg(short, long, default_value = "schedulerdb.sqlite")]
    pub database: String,

    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Subcommand)]
pub enum Commands {
    /// Dump grading results for all images
    DumpGrading {
        /// Show only specific grading status (pending, accepted, rejected)
        #[arg(short, long)]
        status: Option<String>,
        
        /// Filter by project name
        #[arg(short, long)]
        project: Option<String>,
        
        /// Filter by target name
        #[arg(short, long)]
        target: Option<String>,
        
        /// Output format (json, csv, table)
        #[arg(short, long, default_value = "table")]
        format: String,
    },
    
    /// List all projects
    ListProjects,
    
    /// List targets for a specific project
    ListTargets {
        /// Project ID or name
        project: String,
    },
    
    /// Filter rejected files and move them to LIGHT_REJECT folders
    FilterRejected {
        /// Database file to use
        database: String,
        
        /// Base directory containing the image files
        base_dir: String,
        
        /// Perform a dry run (show what would be moved without actually moving)
        #[arg(long)]
        dry_run: bool,
        
        /// Filter by project name
        #[arg(short, long)]
        project: Option<String>,
        
        /// Filter by target name
        #[arg(short, long)]
        target: Option<String>,
        
        #[command(flatten)]
        stat_options: StatisticalOptions,
    },
    
    /// Regrade images in the database based on statistical analysis
    Regrade {
        /// Database file to use
        database: String,
        
        /// Perform a dry run (show what would be changed without actually updating)
        #[arg(long)]
        dry_run: bool,
        
        /// Filter by target name
        #[arg(short, long)]
        target: Option<String>,
        
        /// Filter by project name
        #[arg(short, long)]
        project: Option<String>,
        
        /// Number of days to look back (default: 90)
        #[arg(long, default_value = "90")]
        days: u32,
        
        /// Reset mode: automatic, all, or none (default: none)
        #[arg(long, default_value = "none")]
        reset: String,
        
        #[command(flatten)]
        stat_options: StatisticalOptions,
    },
}

#[derive(Parser, Debug, Clone)]
pub struct StatisticalOptions {
    /// Enable statistical analysis
    #[arg(long)]
    pub enable_statistical: bool,
    
    /// Enable HFR outlier detection
    #[arg(long, requires = "enable_statistical")]
    pub stat_hfr: bool,
    
    /// Standard deviations for HFR outlier detection
    #[arg(long, default_value = "2.0", requires = "stat_hfr")]
    pub hfr_stddev: f64,
    
    /// Enable star count outlier detection
    #[arg(long, requires = "enable_statistical")]
    pub stat_stars: bool,
    
    /// Standard deviations for star count outlier detection
    #[arg(long, default_value = "2.0", requires = "stat_stars")]
    pub star_stddev: f64,
    
    /// Enable distribution analysis (median/mean shift detection)
    #[arg(long, requires = "enable_statistical")]
    pub stat_distribution: bool,
    
    /// Percentage threshold for median shift from mean (0.0-1.0)
    #[arg(long, default_value = "0.1", requires = "stat_distribution")]
    pub median_shift_threshold: f64,
    
    /// Enable cloud detection (sudden rises in median HFR or drops in star count)
    #[arg(long, requires = "enable_statistical")]
    pub stat_clouds: bool,
    
    /// Percentage threshold for cloud detection (0.0-1.0, e.g. 0.2 = 20% change)
    #[arg(long, default_value = "0.2", requires = "stat_clouds")]
    pub cloud_threshold: f64,
    
    /// Number of images needed to establish baseline after cloud event
    #[arg(long, default_value = "5", requires = "stat_clouds")]
    pub cloud_baseline_count: usize,
}

impl StatisticalOptions {
    pub fn to_grading_config(&self) -> Option<crate::grading::StatisticalGradingConfig> {
        if self.enable_statistical {
            Some(crate::grading::StatisticalGradingConfig {
                enable_hfr_analysis: self.stat_hfr,
                hfr_stddev_threshold: self.hfr_stddev,
                enable_star_count_analysis: self.stat_stars,
                star_count_stddev_threshold: self.star_stddev,
                enable_distribution_analysis: self.stat_distribution,
                median_shift_threshold: self.median_shift_threshold,
                enable_cloud_detection: self.stat_clouds,
                cloud_threshold: self.cloud_threshold,
                cloud_baseline_count: self.cloud_baseline_count,
            })
        } else {
            None
        }
    }
}