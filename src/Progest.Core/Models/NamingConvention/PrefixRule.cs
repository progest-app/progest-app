namespace Progest.Core.Models.NamingConvention;

/// <summary>
/// Represents a prefix rule for naming conventions.
/// </summary>
public class PrefixRule
{
    /// <summary>
    /// Gets the type of prefix.
    /// </summary>
    public PrefixType Type { get; }

    /// <summary>
    /// Gets the fixed string value when Type is Fixed.
    /// </summary>
    public string? FixedValue { get; }

    /// <summary>
    /// Gets the date format when Type is Date.
    /// </summary>
    public DateFormat? DateFormat { get; }

    /// <summary>
    /// Initializes a new instance of the PrefixRule class with no prefix.
    /// </summary>
    /// <param name="type">The type of prefix (must be None).</param>
    public PrefixRule(PrefixType type)
    {
        if (type != PrefixType.None)
        {
            throw new ArgumentException($"Prefix type {type} requires additional parameters.");
        }

        Type = type;
    }

    /// <summary>
    /// Initializes a new instance of the PrefixRule class with a fixed value.
    /// </summary>
    /// <param name="type">The type of prefix (must be Fixed).</param>
    /// <param name="fixedValue">The fixed string value to use as prefix.</param>
    public PrefixRule(PrefixType type, string fixedValue)
    {
        if (type != PrefixType.Fixed)
        {
            throw new ArgumentException($"Prefix type {type} does not accept a fixed value parameter.");
        }

        if (string.IsNullOrWhiteSpace(fixedValue))
        {
            throw new ArgumentException("Fixed value cannot be null or whitespace.", nameof(fixedValue));
        }

        Type = type;
        FixedValue = fixedValue;
    }

    /// <summary>
    /// Initializes a new instance of the PrefixRule class with a date format.
    /// </summary>
    /// <param name="type">The type of prefix (must be Date).</param>
    /// <param name="dateFormat">The date format to use for the prefix.</param>
    public PrefixRule(PrefixType type, DateFormat dateFormat)
    {
        if (type != PrefixType.Date)
        {
            throw new ArgumentException($"Prefix type {type} does not accept a date format parameter.");
        }

        Type = type;
        DateFormat = dateFormat;
    }

    /// <summary>
    /// Generates the prefix string based on the rule type.
    /// </summary>
    /// <returns>The generated prefix string.</returns>
    public string GeneratePrefix()
    {
        return Type switch
        {
            PrefixType.None => string.Empty,
            PrefixType.Fixed => FixedValue ?? string.Empty,
            PrefixType.Date => GenerateDatePrefix(),
            _ => throw new InvalidOperationException($"Unknown prefix type: {Type}")
        };
    }

    private string GenerateDatePrefix()
    {
        var now = DateTime.Now;

        return DateFormat switch
        {
            Models.NamingConvention.DateFormat.IsoDate => now.ToString("yyyyMMdd"),
            Models.NamingConvention.DateFormat.IsoDateTime => now.ToString("yyyyMMdd_HHmm"),
            Models.NamingConvention.DateFormat.ReverseDate => now.ToString("yyyy-MM-dd"),
            Models.NamingConvention.DateFormat.ShortDate => now.ToString("yyMMdd"),
            _ => throw new InvalidOperationException($"Unknown date format: {DateFormat}")
        };
    }
}
