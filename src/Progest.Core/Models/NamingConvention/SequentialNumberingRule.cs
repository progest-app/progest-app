namespace Progest.Core.Models.NamingConvention;

/// <summary>
/// Represents a sequential numbering rule for batch file naming operations.
/// </summary>
public class SequentialNumberingRule
{
    /// <summary>
    /// Gets the starting number for sequential numbering.
    /// </summary>
    public int StartNumber { get; }

    /// <summary>
    /// Gets the number of digits to pad the number with.
    /// </summary>
    public int DigitCount { get; }

    /// <summary>
    /// Gets the separator to place after the number.
    /// </summary>
    public string Separator { get; }

    /// <summary>
    /// Initializes a new instance of the SequentialNumberingRule class with default values.
    /// </summary>
    public SequentialNumberingRule() : this(startNumber: 0, digitCount: 4, separator: "_")
    {
    }

    /// <summary>
    /// Initializes a new instance of the SequentialNumberingRule class.
    /// </summary>
    /// <param name="startNumber">The starting number for sequential numbering.</param>
    /// <param name="digitCount">The number of digits to pad the number with.</param>
    /// <param name="separator">The separator to place after the number.</param>
    public SequentialNumberingRule(int startNumber = 0, int digitCount = 4, string separator = "_")
    {
        if (startNumber < 0)
        {
            throw new ArgumentException("Start number cannot be negative.", nameof(startNumber));
        }

        if (digitCount <= 0)
        {
            throw new ArgumentException("Digit count must be positive.", nameof(digitCount));
        }

        if (separator == null)
        {
            throw new ArgumentNullException(nameof(separator));
        }

        StartNumber = startNumber;
        DigitCount = digitCount;
        Separator = separator;
    }

    /// <summary>
    /// Formats a number according to the sequential numbering rule.
    /// </summary>
    /// <param name="number">The number to format.</param>
    /// <returns>The formatted number string with separator.</returns>
    public string Format(int number)
    {
        if (number < 0)
        {
            throw new ArgumentException("Number cannot be negative.", nameof(number));
        }

        var formatSpecifier = new string('0', DigitCount);
        return number.ToString(formatSpecifier) + Separator;
    }
}
