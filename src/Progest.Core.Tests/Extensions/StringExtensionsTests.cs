using FluentAssertions;
using Xunit;
using Progest.Core.Extensions;

namespace Progest.Core.Tests.Extensions;

public class StringExtensionsTests
{
    public class ToSnakeCase
    {
        [Fact]
        public void WithPascalCase_ShouldConvert()
        {
            // Arrange
            var input = "MyClassName";

            // Act
            var result = input.ToSnakeCase();

            // Assert
            result.Should().Be("my_class_name");
        }

        [Fact]
        public void WithCamelCase_ShouldConvert()
        {
            // Arrange
            var input = "myVariableName";

            // Act
            var result = input.ToSnakeCase();

            // Assert
            result.Should().Be("my_variable_name");
        }

        [Fact]
        public void WithKebabCase_ShouldConvert()
        {
            // Arrange
            var input = "my-class-name";

            // Act
            var result = input.ToSnakeCase();

            // Assert
            result.Should().Be("my_class_name");
        }

        [Fact]
        public void WithSpaces_ShouldConvert()
        {
            // Arrange
            var input = "My Class Name";

            // Act
            var result = input.ToSnakeCase();

            // Assert
            result.Should().Be("my_class_name");
        }

        [Fact]
        public void WithEmptyString_ShouldReturnEmpty()
        {
            // Arrange
            var input = "";

            // Act
            var result = input.ToSnakeCase();

            // Assert
            result.Should().Be("");
        }

        [Fact]
        public void WithSingleWord_ShouldReturnLowercase()
        {
            // Arrange
            var input = "Hello";

            // Act
            var result = input.ToSnakeCase();

            // Assert
            result.Should().Be("hello");
        }

        [Fact]
        public void WithNumbers_ShouldPreserve()
        {
            // Arrange
            var input = "File2HtmlConverter";

            // Act
            var result = input.ToSnakeCase();

            // Assert
            result.Should().Be("file2_html_converter");
        }
    }

    public class ToCamelCase
    {
        [Fact]
        public void WithPascalCase_ShouldConvert()
        {
            // Arrange
            var input = "MyClassName";

            // Act
            var result = input.ToCamelCase();

            // Assert
            result.Should().Be("myClassName");
        }

        [Fact]
        public void WithSnakeCase_ShouldConvert()
        {
            // Arrange
            var input = "my_class_name";

            // Act
            var result = input.ToCamelCase();

            // Assert
            result.Should().Be("myClassName");
        }

        [Fact]
        public void WithKebabCase_ShouldConvert()
        {
            // Arrange
            var input = "my-class-name";

            // Act
            var result = input.ToCamelCase();

            // Assert
            result.Should().Be("myClassName");
        }

        [Fact]
        public void WithSpaces_ShouldConvert()
        {
            // Arrange
            var input = "my class name";

            // Act
            var result = input.ToCamelCase();

            // Assert
            result.Should().Be("myClassName");
        }

        [Fact]
        public void WithEmptyString_ShouldReturnEmpty()
        {
            // Arrange
            var input = "";

            // Act
            var result = input.ToCamelCase();

            // Assert
            result.Should().Be("");
        }

        [Fact]
        public void WithSingleWord_ShouldReturnLowercase()
        {
            // Arrange
            var input = "Hello";

            // Act
            var result = input.ToCamelCase();

            // Assert
            result.Should().Be("hello");
        }
    }

    public class ToPascalCase
    {
        [Fact]
        public void WithCamelCase_ShouldConvert()
        {
            // Arrange
            var input = "myClassName";

            // Act
            var result = input.ToPascalCase();

            // Assert
            result.Should().Be("MyClassName");
        }

        [Fact]
        public void WithSnakeCase_ShouldConvert()
        {
            // Arrange
            var input = "my_class_name";

            // Act
            var result = input.ToPascalCase();

            // Assert
            result.Should().Be("MyClassName");
        }

        [Fact]
        public void WithKebabCase_ShouldConvert()
        {
            // Arrange
            var input = "my-class-name";

            // Act
            var result = input.ToPascalCase();

            // Assert
            result.Should().Be("MyClassName");
        }

        [Fact]
        public void WithSpaces_ShouldConvert()
        {
            // Arrange
            var input = "my class name";

            // Act
            var result = input.ToPascalCase();

            // Assert
            result.Should().Be("MyClassName");
        }

        [Fact]
        public void WithEmptyString_ShouldReturnEmpty()
        {
            // Arrange
            var input = "";

            // Act
            var result = input.ToPascalCase();

            // Assert
            result.Should().Be("");
        }

        [Fact]
        public void WithSingleWord_ShouldReturnCapitalized()
        {
            // Arrange
            var input = "hello";

            // Act
            var result = input.ToPascalCase();

            // Assert
            result.Should().Be("Hello");
        }
    }

    public class ToKebabCase
    {
        [Fact]
        public void WithPascalCase_ShouldConvert()
        {
            // Arrange
            var input = "MyClassName";

            // Act
            var result = input.ToKebabCase();

            // Assert
            result.Should().Be("my-class-name");
        }

        [Fact]
        public void WithCamelCase_ShouldConvert()
        {
            // Arrange
            var input = "myClassName";

            // Act
            var result = input.ToKebabCase();

            // Assert
            result.Should().Be("my-class-name");
        }

        [Fact]
        public void WithSnakeCase_ShouldConvert()
        {
            // Arrange
            var input = "my_class_name";

            // Act
            var result = input.ToKebabCase();

            // Assert
            result.Should().Be("my-class-name");
        }

        [Fact]
        public void WithSpaces_ShouldConvert()
        {
            // Arrange
            var input = "My Class Name";

            // Act
            var result = input.ToKebabCase();

            // Assert
            result.Should().Be("my-class-name");
        }

        [Fact]
        public void WithEmptyString_ShouldReturnEmpty()
        {
            // Arrange
            var input = "";

            // Act
            var result = input.ToKebabCase();

            // Assert
            result.Should().Be("");
        }

        [Fact]
        public void WithSingleWord_ShouldReturnLowercase()
        {
            // Arrange
            var input = "Hello";

            // Act
            var result = input.ToKebabCase();

            // Assert
            result.Should().Be("hello");
        }
    }

    public class ToTitleCase
    {
        [Fact]
        public void WithCamelCase_ShouldConvert()
        {
            // Arrange
            var input = "myClassName";

            // Act
            var result = input.ToTitleCase();

            // Assert
            result.Should().Be("My Class Name");
        }

        [Fact]
        public void WithPascalCase_ShouldConvert()
        {
            // Arrange
            var input = "MyClassName";

            // Act
            var result = input.ToTitleCase();

            // Assert
            result.Should().Be("My Class Name");
        }

        [Fact]
        public void WithSnakeCase_ShouldConvert()
        {
            // Arrange
            var input = "my_class_name";

            // Act
            var result = input.ToTitleCase();

            // Assert
            result.Should().Be("My Class Name");
        }

        [Fact]
        public void WithKebabCase_ShouldConvert()
        {
            // Arrange
            var input = "my-class-name";

            // Act
            var result = input.ToTitleCase();

            // Assert
            result.Should().Be("My Class Name");
        }

        [Fact]
        public void WithEmptyString_ShouldReturnEmpty()
        {
            // Arrange
            var input = "";

            // Act
            var result = input.ToTitleCase();

            // Assert
            result.Should().Be("");
        }

        [Fact]
        public void WithSingleWord_ShouldReturnCapitalized()
        {
            // Arrange
            var input = "hello";

            // Act
            var result = input.ToTitleCase();

            // Assert
            result.Should().Be("Hello");
        }
    }
}
