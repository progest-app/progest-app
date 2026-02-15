namespace Progest.Core.Models.NamingConvention;

/// <summary>
/// Defines date format options for prefixes.
/// </summary>
public enum DateFormat
{
    /// <summary>
    /// ISO 8601 format: yyyyMMdd
    /// </summary>
    IsoDate,

    /// <summary>
    /// ISO 8601 format with time: yyyyMMdd_HHmm
    /// </summary>
    IsoDateTime,

    /// <summary>
    /// Reverse date format: yyyy-MM-dd
    /// </summary>
    ReverseDate,

    /// <summary>
    /// Short year format: yyMMdd
    /// </summary>
    ShortDate
}
